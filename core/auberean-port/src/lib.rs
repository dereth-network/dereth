//! The marker for retail client functions the rebuild has not ported yet.
//!
//! **Depends on** nothing. **Used by** the script machine (`auberean-script`) and its natives
//! (`auberean-glue`).
//!
//! Every function of the retail client has an opaque id, five digits after a two-letter prefix
//! (`D2-00001` to `D2-18973`). A Rust item that ports one carries a line comment that is only that
//! id, `// D2-04211` (several ids comma-separated), on the line before the item. A place where a
//! retail function belongs but has not been written yet calls [`not_ported!`] with its id, so one
//! rule finds both.
//!
//! **Must never** stand for anything but a function not written yet: reaching one panics with its
//! id.

#![no_std]

/// Stands in for a retail client function that is not ported yet. Panics with its id.
///
/// ```should_panic
/// fn placeholder() -> u32 {
///     auberean_port::not_ported!("D2-00000")
/// }
/// placeholder();
/// ```
#[macro_export]
macro_rules! not_ported {
    ($id:literal) => {
        ::core::panic!(::core::concat!("not ported: ", $id))
    };
}
