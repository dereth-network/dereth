//! The wire format: `ProtoHeader`, the 22 flag bits, the 19 optional headers, the fragment header
//! and packet assembly.
//!
//! ```text
//! +---------------------------------------------+  offset 0
//! |  ProtoHeader                      20 bytes  |
//! +---------------------------------------------+  offset 20
//! |  optional header sections, ascending flag   |
//! |  order, back to back, no padding            |
//! +---------------------------------------------+
//! |  blob fragment 0 : 16-byte header + payload |
//! |  ...                                        |
//! +---------------------------------------------+  offset 20 + datalen
//! ```
//!
//! See `docs/networking/01-packet-format.md` §§1-4.

pub mod frag;
pub mod header;
pub(crate) mod le;
pub mod optional;
pub mod packet;

pub use frag::{Fragment, FragmentHeader};
pub use header::{PacketFlags, ProtoHeader};
pub use optional::{OptionalHeaderSpec, SectionLen, OPTIONAL_HEADERS};
pub use packet::{OutPacket, ParsedPacket};

/// `sizeof(ProtoHeader)`, packed with no padding.
pub const HEADER_SIZE: usize = 20;

/// The fragment header's size.
pub const FRAG_HEADER_SIZE: usize = 16;

/// Maximum fragment *payload*, `0x1C0`.
///
pub const MAX_FRAG_DATA: usize = 448;

/// Maximum *whole* fragment including its 16-byte header, `0x1D0`.
///
/// The fragment constructor accepts `0x10 <= blob_frag_size < 0x1D1`.
pub const MAX_FRAG_SIZE: usize = 464;

/// The largest post-header payload the client will ever *build*, `0x1D0`.
///
/// The coalescer appends a fragment only while
/// `packet size + fragment size < 0x1D1`.
pub const MAX_BUILD_PAYLOAD: usize = 464;

/// The largest datagram the client will ever *send*: 20 + 464.
///
/// It *accepts* up to 65,524. Do not raise this even though the receiver tolerates more: ACE's
/// receive buffer is 1024 and retail's is unknown.
pub const MAX_DATAGRAM_OUT: usize = HEADER_SIZE + MAX_BUILD_PAYLOAD;

/// `recvfrom` length in.
pub const RECV_BUFFER_SIZE: usize = 65504;

/// The first `datalen` value rejects.
pub const DATALEN_REJECT_AT: u16 = 0xFFE1;

/// Maximum optional-header sections in one packet.
pub const MAX_OPTIONAL_HEADERS: usize = 32;

/// Maximum ordinary fragments in one packet.
pub const MAX_FRAGS_PER_PACKET: usize = 29;

/// Why a datagram could not be parsed.
///
/// Every variant corresponds to a specific rejection in the header verification or the packet
/// parse. The client logs none of them; a rejected packet simply
/// vanishes on both sides, which is why they are enumerated here rather than collapsed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum WireError {
    #[error("datagram is {0} bytes, shorter than the 20-byte ProtoHeader")]
    TooShortForHeader(usize),
    #[error("datalen_ = {0} is at or above the 0xFFE1 rejection threshold")]
    DatalenTooLarge(u16),
    #[error("datalen_ = {declared} but {actual} bytes follow the header")]
    DatalenMismatch { declared: u16, actual: usize },
    #[error("recipient id {0} does not fit the 256-entry receiver table")]
    RecIdOutOfRange(u16),
    #[error("header_ carries undefined bit(s) {0:#010X} at or above 0x100")]
    UndefinedFlagBits(u32),
    #[error("optional header {mask:#010X} needs {need} bytes, {have} remain")]
    SectionTruncated { mask: u32, need: usize, have: usize },
    #[error("optional header {mask:#010X} declares {count} sequence ids, the cap is 114")]
    SeqIdListTooLong { mask: u32, count: u32 },
    #[error("fragment declares size {0}, outside 16..=464")]
    FragmentSizeOutOfRange(u16),
    #[error("fragment declares size {need} but {have} bytes remain")]
    FragmentTruncated { need: usize, have: usize },
    #[error("{0} unconsumed byte(s) after parsing; the client requires an exact fit")]
    TrailingBytes(usize),
    #[error("packet carries fragments but header_ bit 0x4 is clear, or the converse")]
    FragmentFlagMismatch,
    #[error(
        "EncryptedChecksum ({encrypted}) disagrees with needs-encryption ({needs_encryption}): \
         a packet is encrypted iff it carries fragments or a non-disposable optional header"
    )]
    EncryptionEquivalence {
        encrypted: bool,
        needs_encryption: bool,
    },
    #[error("encrypted packet has sequence number 0")]
    EncryptedWithZeroSequence,
    #[error("optional header {0:#010X} is exclusive but the packet carries other sections")]
    ExclusiveHeaderNotAlone(u32),
    #[error("more than 32 optional headers, or more than 29 fragments, in one packet")]
    TooManySections,
    #[error("a PString runs past the end of its section")]
    MalformedPString,
}
