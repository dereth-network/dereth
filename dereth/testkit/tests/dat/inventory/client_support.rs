use super::*;

/// A whole client over the retail data, with nothing in its world yet.
///
/// The dats are what this tier is for: the attachment the three claims below are about is
/// refused outright by a client built without them.
pub(super) fn a_client() -> HeadlessClient {
    HeadlessClient::new(ClientSpec::retail())
}

/// A whole client with its shell up and the gameplay screen settled: the tree these scenarios
/// read their answers off.
pub(super) fn a_gameplay_client() -> HeadlessClient {
    HeadlessClient::new(ClientSpec::gameplay(4))
}

/// A client over the retail data, with no shell: what these scenarios want from it is the dat
/// store the terrain comes out of, and the place to book the claim.
pub(super) fn a_retail_client() -> HeadlessClient {
    HeadlessClient::new(ClientSpec::retail())
}
