use super::{FeedbackKind, FeedbackSeverity};
use crate::chat::failure::handle_failure_event;

/// Behaviour: feedback.numeric.the-producing-arm-keeps-its-code-route-and-established-emphasis
#[test]
fn numeric_failure_lines_keep_their_code_route_and_established_emphasis() {
    let cases = [
        (0x0402, 7, "Your spell fizzled.\n", None),
        (
            0x0403,
            0x1a,
            "Your spell's target is missing!",
            Some(FeedbackSeverity::Warning),
        ),
        (
            0x0404,
            0x1a,
            "Your projectile spell mislaunched!",
            Some(FeedbackSeverity::Warning),
        ),
        (
            0x0407,
            0x1a,
            "Your spell cannot be cast outside",
            Some(FeedbackSeverity::Warning),
        ),
        (
            0x052a,
            0x1a,
            "That is not a salvaging tool.",
            Some(FeedbackSeverity::Warning),
        ),
        (0x0529, 0x1a, "Trade Complete!", None),
    ];
    for (code, channel, body, severity) in cases {
        let line = handle_failure_event(code, "unused substitution").expect("displayed arm");
        assert_eq!(line.feedback.kind, FeedbackKind::NumericFailure(code));
        assert_eq!(line.feedback.severity, severity, "code {code:#06x}");
        assert_eq!(line.ty, channel, "code {code:#06x}");
        assert_eq!(line.body, body, "code {code:#06x}");
        assert_eq!(line.window, 0);
        assert!(line.prefix.is_none());
    }
    let joined = handle_failure_event(0x051b, "General").expect("channel announcement");
    assert_eq!(joined.feedback.kind, FeedbackKind::NumericFailure(0x051b));
    assert_eq!(joined.feedback.severity, None);
    assert_eq!(joined.ty, 0);
    assert_eq!(joined.body, "You have entered the General channel.\n");
}

/// Behaviour: feedback.numeric.silent-and-unknown-arms-do-not-create-a-displayed-line
#[test]
fn silent_and_unknown_numeric_arms_do_not_create_a_displayed_line() {
    for code in [0x04b8, 0x04b9, 0x04ba, 0, 0x0417, u32::MAX] {
        assert!(
            handle_failure_event(code, "That is not a salvaging tool.").is_none(),
            "code {code:#06x} must not acquire a line from its supplied text"
        );
    }
}
