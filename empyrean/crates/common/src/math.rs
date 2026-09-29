//! The server's single entry point for transcendental functions. The functions themselves are
//! `dereth_primitives::num::math`, shared with the client and named
//! directly as `dereth_primitives::num`, so the client's physics and the server's ports round alike; this module
//! re-exports them and records how ACE's calls map onto them.
//!
//! ACE calls `Math.Sin`, `Math.Pow`, `MathF.Sin` and the rest, which .NET implements as calls into
//! the host's C runtime (dotnet/runtime `src/coreclr/classlibnative/float/floatdouble.cpp` and
//! `floatsingle.cpp`: `COMDouble::Pow` returns `pow(x, y)`, `COMSingle::Sin` returns `sinf(x)`, and
//! so on). ACE's results therefore depend on its host: the vectors were recorded on Windows (the
//! UCRT). Rust's `f64::sin` and friends are also host calls, so calling them directly would give
//! different results on Windows, Linux (glibc) and macOS.
//!
//! By default every function here is **portable**: pure Rust, the same bits on every host and
//! architecture, chosen per function as the candidate closest to the UCRT over the argument ranges
//! ACE uses (measured with a standalone fidelity tool):
//!
//! * `pxfm` (BSD-3-Clause OR Apache-2.0; pure-Rust ports of CORE-MATH and LLVM libc, correctly
//!   rounded): `sinf`, `cosf`, `atan2`, `exp`, `log`, `pow`, and `hypot`/`hypotf` (no ACE
//!   counterpart; the soak bots' own geometry). A correctly rounded result does not depend on
//!   how it is computed, so it is the same whether `pxfm` takes its FMA path (chosen at run time
//!   on x86, always on ARM) or not; the measurements found no difference.
//! * `libm` (MIT; the Rust port of musl, itself from FreeBSD's msun/fdlibm): `sin`, `cos`, `acos`,
//!   whose UCRT versions are faithfully rounded and agree with fdlibm's more often than with the
//!   correctly rounded result. `libm` uses only IEEE basic operations, never FMA, so it is the
//!   same everywhere.
//!
//! The cargo feature `host-libm` switches to the host C runtime (through `std`),
//! which is bit-exact with ACE on Windows and host-dependent elsewhere. It forwards to
//! `dereth-primitives/host-libm`, so the shared crates switch with it.
//!
//! ACE's calls map as: `MathF.Sin`/`Cos` to [`sinf`]/[`cosf`]; `MathF.SinCos` (`float.SinCos`,
//! which net10's `Quaternion.CreateFromAxisAngle` calls with the half angle) to [`sin_cosf`];
//! `Math.Sin`/`Cos`/`Acos`/`Atan2`/`Exp`/`Pow` to [`sin`]/[`cos`]/[`acos`]/[`atan2`]/[`exp`]/[`pow`];
//! `Math.Log(x)` to [`log`](fn@log) (`Math.Log(a, b)` is `Log(a) / Log(b)` in .NET). [`hypot`]/[`hypotf`]
//! have no ACE counterpart (the soak bots' own geometry).
//!
//! NaN results are always the one quiet NaN (`0x7FF8...`/`0x7FC00000`), whatever the input.
//!
//! `sqrt` is not here: IEEE 754 requires it to be correctly rounded, and it is on every host.
//!
//! DIVERGE: the portable functions are not always the UCRT's last bit, so ACE's Windows-recorded
//! vectors differ in a few cases (recorded in DIVERGENCES.md). The rest of every vector set is
//! bit-exact.

pub use dereth_primitives::num::math::*;
