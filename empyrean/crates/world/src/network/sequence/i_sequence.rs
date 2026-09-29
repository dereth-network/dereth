// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/Sequence/ISequence.cs
//! Port of `Source/ACE.Server/Network/Sequence/ISequence.cs`.

// ACE: ISequence
/// ACE `ISequence`: a wrapping counter that hands out its value as little-endian bytes.
///
/// `NextBytes` is a C# property whose getter advances the counter, so it takes `&mut self`.
pub trait ISequence: std::fmt::Debug + Send {
    /// `NextBytes`: advance, then the new value's bytes.
    fn next_bytes(&mut self) -> Vec<u8>;
    /// `CurrentBytes`: the current value's bytes.
    fn current_bytes(&self) -> Vec<u8>;
}
