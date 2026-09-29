//! One datagram in one WebSocket message: the shared frame, `dereth_transport::web_frame`, at the
//! path this crate names it by. A server with a WebSocket endpoint and `dereth-web-relay` both take
//! it, so the client has one path; it is specified in `docs/networking/05-websocket-frame.md`.

pub use dereth_transport::web_frame::*;
