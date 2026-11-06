//! The relay's whole exchange, in process: the later protocol's real client transport on one side, a
//! stand-in for the ac1999 server on the other, and the relay between them on a virtual clock.
//! Behaviour: none (a synthetic server speaks from the relay's own codecs; no retail bytes).

mod exchange;
mod standin;
