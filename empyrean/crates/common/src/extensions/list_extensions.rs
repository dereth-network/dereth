// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Common/Extensions/ListExtensions.cs
//! `ListExtensions`.

use crate::thread_safe_random::ThreadSafeRandom;

// ACE: ListExtensions.Shuffle
/// Fisher-Yates from the back, drawing `ThreadSafeRandom.Next(0, n)` (inclusive) per step.
pub fn shuffle<T>(list: &mut [T]) {
    let mut n = i32::try_from(list.len()).expect("IList.Count is an int");
    while n > 1 {
        n -= 1;
        let k = ThreadSafeRandom::next(0, n);
        let (k, n) = (
            usize::try_from(k).unwrap_or(0),
            usize::try_from(n).unwrap_or(0),
        );
        list.swap(k, n);
    }
}

// ACE: ListExtensions.Product
/// The product of the list in `float`, left to right from 1.
#[must_use]
pub fn product(list: &[f32]) -> f32 {
    let mut total_product = 1.0f32;
    for item in list {
        total_product *= item;
    }
    total_product
}
