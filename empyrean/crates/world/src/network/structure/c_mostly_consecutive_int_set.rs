// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/Structure/CMostlyConsecutiveIntSet.cs
//! Port of `Source/ACE.Server/Network/Structure/CMostlyConsecutiveIntSet.cs`.

use empyrean_common::dotnet::binary_reader::{BinaryReader, ReadError};

// ACE: CMostlyConsecutiveIntSet, CMostlyConsecutiveIntSet.CMostlyConsecutiveIntSet
/// `new CMostlyConsecutiveIntSet()` is `Default`.
/// A run-length-encoded set of dat iterations: a negative entry stands for a run.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CMostlyConsecutiveIntSet {
    // ACE: CMostlyConsecutiveIntSet.Iterations
    pub iterations: i32,
    // ACE: CMostlyConsecutiveIntSet.Ints
    pub ints: Vec<i32>,
}

impl std::fmt::Display for CMostlyConsecutiveIntSet {
    // ACE: CMostlyConsecutiveIntSet.ToString
    /// `Environment.NewLine` is `"\r\n"` on the Windows host ACE runs on.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut str = String::new();

        str += &format!("Iterations:  {}\r\n", self.iterations);
        str += "Ints:\r\n";

        for i in &self.ints {
            str += &format!("  |   {i}\r\n");
        }

        f.write_str(&str)
    }
}

// ACE: CMostlyConsecutiveIntSetExtensions.ReadCMostlyConsecutiveIntSet
/// `reader.ReadCMostlyConsecutiveIntSet()`: reads entries until the running iteration count
/// equals `Iterations` (a run `x < 0` counts `|x| - 1`). Input that never lands exactly on
/// `Iterations` reads until the stream ends, as ACE does. `Err` is ACE's exception: the
/// `EndOfStreamException`, or the `OverflowException` of `Math.Abs(int.MinValue)` (reported as an
/// end of stream with nothing needed).
pub fn read_c_mostly_consecutive_int_set(
    reader: &mut BinaryReader<'_>,
) -> Result<CMostlyConsecutiveIntSet, ReadError> {
    let mut new_obj = CMostlyConsecutiveIntSet {
        iterations: reader.read_i32()?,
        ints: Vec::new(),
    };
    let mut iterations: i32 = 0;
    while iterations != new_obj.iterations {
        let x = reader.read_i32()?;
        if x < 0 {
            let Some(x_abs) = x.checked_abs() else {
                return Err(ReadError::EndOfStream {
                    at: reader.position(),
                    needed: 0,
                    available: 0,
                });
            };
            iterations = iterations.wrapping_add(x_abs - 1);
        } else {
            iterations = iterations.wrapping_add(1);
        }

        new_obj.ints.push(x);
    }
    Ok(new_obj)
}
