//! The housing purchase wait.
//!
//! One constant shared with `dereth_ui_screens::panels::house`: the thirty days
//! `dereth_client::hud` adds to int quality `0xC7` when it answers the panel's purchase-time line.

/// The purchase wait period, and the offset the house panel adds to the purchase timestamp
/// before it draws the "you may buy again" line — thirty days.
pub const PURCHASE_WAIT_SECONDS: i64 = 0x0027_8D00;
