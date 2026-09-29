//! .NET semantics that ported ACE code depends on.
//!
//! Nothing here is ported from ACE: these are clean-room models of the .NET runtime behaviours
//! that ACE's code relies on, each citing the runtime source it follows. Ported code uses them
//! wherever ACE's result would otherwise differ from the obvious Rust spelling:
//!
//! * [`math`]: `Math.Round` (banker's by default), `Math.Max`/`Math.Min` (NaN-propagating);
//! * [`console`]: `Console.WriteLine` and `Console.Write`, which the port writes to the server log;
//! * [`cast`]: C# numeric casts ([`CsCast`]), including net10.0's saturating float→int casts;
//! * [`dict`]: [`DotNetDict`] and [`DotNetHashSet`], which enumerate in .NET's order;
//! * [`format`](mod@format): numeric `ToString(format)` in `en-US`;
//! * [`datetime`]: [`DotNetDateTime`] and [`TimeSpan`];
//! * [`decimal`]: the exact `decimal` sums and conversions ACE's chance arithmetic uses;
//! * [`sort`]: `List<T>.Sort(Comparison<T>)` (the unstable introsort) and `CompareTo`;
//! * [`numerics`]: `System.Numerics` `Vector2`, `Vector3` and `Quaternion` as net10 computes them;
//! * [`binary_reader`]: the `System.IO.BinaryReader` reads over a byte slice, the one
//!   implementation every server crate's payload reader uses. It also carries ACE.Common's
//!   `BinaryReaderExtensions` (`ReadString16L`, `ReadString32L`), the only ACE-derived code here,
//!   in [`binary_reader_extensions`].
//!
//! The pseudo-random generator (`System.Random`'s seeded algorithm) lives in [`crate::random`].

pub mod binary_reader;
pub mod binary_reader_extensions;
pub mod cast;
pub mod console;
pub mod datetime;
pub mod decimal;
pub mod dict;
pub mod format;
pub mod math;
pub mod numerics;
pub mod sort;

pub use binary_reader::{BinaryReader, ReadError};
pub use cast::CsCast;
pub use datetime::{DotNetDateTime, TimeSpan};
pub use dict::{DotNetDict, DotNetHashSet};
pub use format::{align, format, format_aligned, to_string, Num};
pub use math::MidpointRounding;
pub use numerics::{Quaternion, Vector2, Vector3};
