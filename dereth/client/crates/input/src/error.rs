//! One error enum for the crate. Parsers return `Result`; they do
//! not panic on malformed input, because they will meet malformed input — a `.keymap` file is
//! written by players and edited by hand.

/// Everything `dereth-input` can fail at.
#[derive(Debug, thiserror::Error)]
pub enum InputError {
    /// The dat decoder refused the payload: a short read, bytes left over, or a container
    /// header the client's reader would refuse.
    #[error("dat decode: {0}")]
    Decode(#[from] dereth_assets::AssetError),

    #[error("{what}: got 0x{got:08X}, expected 0x{want:08X}")]
    BadMagic {
        what: &'static str,
        got: u32,
        want: u32,
    },

    #[error("malformed: {0}")]
    Malformed(&'static str),

    /// One of the `.keymap` reader's own validation messages, verbatim from retail. The client
    /// reports these through its error path and then abandons the file.
    #[error("{0}")]
    KeymapFile(String),

    #[error("i/o error: {0}")]
    Io(#[from] std::io::Error),
}
