//! The `a1.` ticket against the vectors: an account and its secret written and read back, the
//! strings that are not of the form, and a ticket's debug form, which hides the secret.

use auberean_friends::{Ticket, TicketError};

use crate::common::{list, text};

/// Behaviour: friends.ticket.a1-form
#[test]
fn a_ticket_is_a1_then_the_account_and_secret_in_url_safe_base64() {
    for v in list("tickets") {
        let t = Ticket::new(text(v, "account"), text(v, "secret"));
        let coded = text(v, "ticket");
        assert_eq!(t.encode().as_deref(), Ok(coded));
        assert_eq!(Ticket::decode(coded), Ok(t.clone()));
        assert!(
            coded
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.')),
            "{coded}: safe on any command line"
        );
        let shown = format!("{t:?}");
        let secret = format!("{:?}", text(v, "secret"));
        assert!(!shown.contains(&secret), "{shown} shows {secret}");
        assert!(shown.contains("\"***\""), "{shown}");
    }
}

/// Behaviour: friends.ticket.a1-form
#[test]
fn strings_not_of_the_a1_form_are_not_tickets() {
    for v in list("bad_tickets") {
        assert!(
            Ticket::decode(text(v, "ticket")).is_err(),
            "{}",
            text(v, "about")
        );
    }
    assert_eq!(Ticket::new("", "x").encode(), Err(TicketError::NoAccount));
    let long = "x".repeat(400);
    assert_eq!(
        Ticket::new("ann", &long).encode(),
        Err(TicketError::TooLong)
    );
}
