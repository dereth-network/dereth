//! Shared text tags and their UI click notices.

pub use dereth_text::tag::*;

use crate::msg::NoticeId;

/// The notice raised when the tag is clicked.
#[must_use]
pub const fn click_notice(kind: TagKind) -> Option<NoticeId> {
    Some(match kind {
        TagKind::Did => NoticeId::DidTagClicked,
        TagKind::Iid => NoticeId::IidTagClicked,
        TagKind::IidEnum => NoticeId::IidEnumTagClicked,
        TagKind::IidString => NoticeId::IidStringTagClicked,
        TagKind::Unknown => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Oracle: the four tag subclasses and their click notices.
    #[test]
    fn each_kind_raises_its_documented_notice() {
        assert_eq!(click_notice(TagKind::Did), Some(NoticeId::DidTagClicked));
        assert_eq!(click_notice(TagKind::Iid), Some(NoticeId::IidTagClicked));
        assert_eq!(
            click_notice(TagKind::IidEnum),
            Some(NoticeId::IidEnumTagClicked)
        );
        assert_eq!(
            click_notice(TagKind::IidString),
            Some(NoticeId::IidStringTagClicked)
        );
        assert_eq!(click_notice(TagKind::Unknown), None);
    }
}
