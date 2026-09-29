// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/Structure/SquelchInfo.cs
//! Port of `Source/ACE.Server/Network/Structure/SquelchInfo.cs`.

use empyrean_entity::enums::SquelchMask;

use crate::network::game_messages::game_message::ace_str;
use crate::network::game_messages::game_message::write_record;

// ACE: SquelchInfo
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SquelchInfo {
    // ACE: SquelchInfo.Filters
    pub filters: Vec<SquelchMask>,
    // ACE: SquelchInfo.PlayerName
    /// `null` for the parameterless constructor (written as an empty string).
    pub player_name: Option<String>,
    // ACE: SquelchInfo.Account
    pub account: bool,
}

impl SquelchInfo {
    // ACE: SquelchInfo.SquelchInfo
    /// `new SquelchInfo()`: no filters.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    // ACE: SquelchInfo.SquelchInfo
    /// `new SquelchInfo(SquelchMask filter, string playerName, bool account)`.
    ///
    /// not sure why this is sent 4x.. if not sent 4x, then the 'checkbox' in the chat menu
    /// doesn't toggle. there doesn't appear to be any pcaps of players performing per-channel
    /// character squelches, so not sure how those were handled for this packet.
    #[must_use]
    pub fn from_filter(filter: SquelchMask, player_name: &str, account: bool) -> Self {
        Self {
            filters: vec![filter, filter, filter, filter],
            player_name: Some(player_name.to_owned()),
            account,
        }
    }

    // ACE: SquelchInfo.SquelchInfo
    /// `new SquelchInfo(List<SquelchMask> filters, string playerName, bool account)`.
    #[must_use]
    pub fn from_filters(
        filters: Vec<SquelchMask>,
        player_name: Option<String>,
        account: bool,
    ) -> Self {
        Self {
            filters,
            player_name,
            account,
        }
    }
}

// ACE: SquelchInfoExtensions.Write
/// `writer.Write(SquelchInfo info)`.
pub fn write(writer: &mut Vec<u8>, info: &SquelchInfo) {
    write_record(writer, &[info.player_name.as_deref().unwrap_or("")], |w| {
        record(info).write(w)
    });
}

// ACE: SquelchInfoExtensions.Write
/// `writer.Write(List<SquelchMask> filters)`.
pub fn write_filters(writer: &mut Vec<u8>, filters: &[SquelchMask]) {
    write_record(writer, &[], |w| filters_record(filters).write(w));
}

/// The dereth-protocol record the `Write` extension below writes, field for field.
#[must_use]
pub fn record(info: &SquelchInfo) -> dereth_protocol::comms::SquelchInfo {
    dereth_protocol::comms::SquelchInfo {
        squelch_msgs: filters_record(&info.filters),
        name: ace_str(info.player_name.as_deref()),
        is_zone_squelch: i32::from(info.account),
    }
}

/// The filter list as dereth-protocol's record (a count, then each mask).
#[must_use]
pub fn filters_record(filters: &[SquelchMask]) -> dereth_protocol::comms::VLong {
    dereth_protocol::comms::VLong(filters.iter().map(|f| f.0).collect())
}
