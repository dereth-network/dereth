//! Vectors: local net10 saturating, narrowing and rounding cast cases in this module
//! C# unchecked casts as net10 x64 runs them: float-to-int truncates and saturates (NaN 0), small
//! ints narrow via int, int casts wrap, to-float rounds to nearest even.
//! Fixture: locally constructed values and deterministic expected results.

use empyrean_common::dotnet::CsCast;

#[test]
fn float_to_int_truncates_toward_zero() {
    assert_eq!(CsCast::<i32>::cs_cast(2.9f64), 2);
    assert_eq!(CsCast::<i32>::cs_cast(-2.9f64), -2);
    assert_eq!(CsCast::<i32>::cs_cast(-0.5f32), 0);
    assert_eq!(CsCast::<u32>::cs_cast(4_294_967_295.9f64), u32::MAX);
    assert_eq!(
        CsCast::<i64>::cs_cast(-9.007_199_254_740_993e15f64),
        -9_007_199_254_740_992
    );
    assert_eq!(
        CsCast::<u64>::cs_cast(1.8e19f64),
        18_000_000_000_000_000_000
    );
}

#[test]
fn float_to_int_saturates_and_nan_is_zero_on_net9_and_later() {
    assert_eq!(CsCast::<i32>::cs_cast(1e10f64), i32::MAX);
    assert_eq!(CsCast::<i32>::cs_cast(-1e10f64), i32::MIN);
    assert_eq!(CsCast::<i32>::cs_cast(f64::NAN), 0);
    assert_eq!(CsCast::<i32>::cs_cast(f32::INFINITY), i32::MAX);
    assert_eq!(
        CsCast::<u32>::cs_cast(-1.0f64),
        0,
        "negative to unsigned clamps to 0"
    );
    assert_eq!(CsCast::<u32>::cs_cast(1e10f32), u32::MAX);
    assert_eq!(CsCast::<u32>::cs_cast(f32::NAN), 0);
    assert_eq!(CsCast::<i64>::cs_cast(1e19f64), i64::MAX);
    assert_eq!(CsCast::<i64>::cs_cast(f64::NEG_INFINITY), i64::MIN);
    assert_eq!(CsCast::<u64>::cs_cast(-5.0f64), 0);
    assert_eq!(CsCast::<u64>::cs_cast(1e30f64), u64::MAX);
    assert_eq!(CsCast::<u64>::cs_cast(f64::NAN), 0);
}

#[test]
fn float_to_small_int_goes_through_int_then_narrows() {
    assert_eq!(CsCast::<u8>::cs_cast(300.0f64), 44);
    assert_eq!(CsCast::<u8>::cs_cast(-1.0f64), 255);
    assert_eq!(CsCast::<u8>::cs_cast(255.9f32), 255);
    assert_eq!(CsCast::<u8>::cs_cast(1e10f64), 255, "int.MaxValue narrowed");
    assert_eq!(CsCast::<u8>::cs_cast(f64::NAN), 0);
    assert_eq!(CsCast::<i8>::cs_cast(200.0f64), -56);
    assert_eq!(CsCast::<i16>::cs_cast(40000.0f64), -25536);
    assert_eq!(CsCast::<u16>::cs_cast(-2.0f32), 65534);
    assert_eq!(CsCast::<u16>::cs_cast(-1e10f64), 0, "int.MinValue narrowed");
}

#[test]
fn integer_casts_wrap() {
    assert_eq!(CsCast::<u32>::cs_cast(-1i32), u32::MAX);
    assert_eq!(CsCast::<i32>::cs_cast(u32::MAX), -1);
    assert_eq!(CsCast::<i32>::cs_cast(0x1_0000_0005i64), 5);
    assert_eq!(CsCast::<u8>::cs_cast(-1i64), 255);
    assert_eq!(CsCast::<i64>::cs_cast(u64::MAX), -1);
    assert_eq!(CsCast::<u64>::cs_cast(-1i32), u64::MAX, "sign extension");
    assert_eq!(CsCast::<u64>::cs_cast(u32::MAX), 4_294_967_295);
    assert_eq!(CsCast::<i16>::cs_cast(0x8000u16), i16::MIN);
}

#[test]
fn casts_to_floating_point_round_to_nearest_even() {
    assert_eq!(CsCast::<f32>::cs_cast(16_777_217i32), 16_777_216.0);
    assert_eq!(CsCast::<f32>::cs_cast(0.1f64), 0.1f32);
    assert_eq!(CsCast::<f64>::cs_cast(0.1f32), f64::from(0.1f32));
    assert_eq!(
        CsCast::<f64>::cs_cast(u64::MAX),
        18_446_744_073_709_551_616.0
    );
    // (float)ulong is conv.r.un then conv.r4: 2^63 + 2^39 + 1 first rounds to 2^63 + 2^39 in
    // double, a tie in float that goes to even (2^63); a direct rounding would give 2^63 + 2^40.
    let v: u64 = (1 << 63) + (1 << 39) + 1;
    assert_eq!(CsCast::<f32>::cs_cast(v), 9_223_372_036_854_775_808.0f32);
    #[allow(clippy::cast_precision_loss)]
    let direct = v as f32;
    assert_ne!(direct, 9_223_372_036_854_775_808.0f32);
    assert_eq!(
        CsCast::<f32>::cs_cast(v.cast_signed()),
        -9_223_371_487_098_961_920.0f32,
        "(float)long rounds once"
    );
}
