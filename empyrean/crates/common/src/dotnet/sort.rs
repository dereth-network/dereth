//! .NET's `List<T>.Sort(Comparison<T>)` and the `CompareTo` results comparisons return.
//!
//! Clean-room models of the runtime (`ArraySortHelper<T>.IntrospectiveSort` in
//! `System.Private.CoreLib`, and `Int32`/`Single`/`Double.CompareTo`), not ACE code.

use std::cmp::Ordering;

/// .NET's `List<T>.Sort(Comparison<T>)`: `ArraySortHelper<T>.IntrospectiveSort`, which is not
/// stable. The order of equal elements is observable (a monster's clothing choice, the side pack
/// an item lands in), so this is the runtime's algorithm step for step; `comparer` returns a
/// `CompareTo` result.
pub fn list_sort<T>(keys: &mut [T], mut comparer: impl FnMut(&T, &T) -> i32) {
    if keys.len() > 1 {
        let depth_limit = 2 * (i32::try_from(keys.len().ilog2()).unwrap_or(i32::MAX) + 1);
        intro_sort(keys, depth_limit, &mut comparer);
    }
}

/// `Array.IntrosortSizeThreshold`.
const INTROSORT_SIZE_THRESHOLD: usize = 16;

fn intro_sort<T>(keys: &mut [T], mut depth_limit: i32, comparer: &mut impl FnMut(&T, &T) -> i32) {
    let mut partition_size = keys.len();
    while partition_size > 1 {
        if partition_size <= INTROSORT_SIZE_THRESHOLD {
            if partition_size == 2 {
                swap_if_greater(keys, comparer, 0, 1);
                return;
            }

            if partition_size == 3 {
                swap_if_greater(keys, comparer, 0, 1);
                swap_if_greater(keys, comparer, 0, 2);
                swap_if_greater(keys, comparer, 1, 2);
                return;
            }

            insertion_sort(&mut keys[..partition_size], comparer);
            return;
        }

        if depth_limit == 0 {
            heap_sort(&mut keys[..partition_size], comparer);
            return;
        }
        depth_limit -= 1;

        let p = pick_pivot_and_partition(&mut keys[..partition_size], comparer);

        // Note we've already partitioned around the pivot and do not have to move the pivot again.
        intro_sort(&mut keys[p + 1..partition_size], depth_limit, comparer);
        partition_size = p;
    }
}

fn swap_if_greater<T>(
    keys: &mut [T],
    comparer: &mut impl FnMut(&T, &T) -> i32,
    i: usize,
    j: usize,
) {
    if comparer(&keys[i], &keys[j]) > 0 {
        keys.swap(i, j);
    }
}

fn pick_pivot_and_partition<T>(keys: &mut [T], comparer: &mut impl FnMut(&T, &T) -> i32) -> usize {
    let hi = keys.len() - 1;

    // Compute median-of-three. But also partition them, since we've done the comparison.
    let middle = hi >> 1;

    // Sort lo, mid and hi appropriately, then pick mid as the pivot.
    swap_if_greater(keys, comparer, 0, middle); // swap the low with the mid point
    swap_if_greater(keys, comparer, 0, hi); // swap the low with the high
    swap_if_greater(keys, comparer, middle, hi); // swap the middle with the high

    // The pivot is compared by position: after `Swap(keys, middle, hi - 1)` it sits at `hi - 1`,
    // which the scans below never move until the final swap.
    keys.swap(middle, hi - 1);
    let pivot = hi - 1;
    let mut left = 0;
    let mut right = hi - 1; // We already partitioned lo and hi and put the pivot in hi - 1. And we pre-increment & decrement below.

    while left < right {
        loop {
            left += 1;
            if comparer(&keys[left], &keys[pivot]) >= 0 {
                break;
            }
        }
        loop {
            right -= 1;
            if comparer(&keys[pivot], &keys[right]) >= 0 {
                break;
            }
        }

        if left >= right {
            break;
        }

        keys.swap(left, right);
    }

    // Put pivot in the right location.
    if left != hi - 1 {
        keys.swap(left, hi - 1);
    }
    left
}

fn heap_sort<T>(keys: &mut [T], comparer: &mut impl FnMut(&T, &T) -> i32) {
    let n = keys.len();
    let mut i = n >> 1;
    while i >= 1 {
        down_heap(keys, i, n, comparer);
        i -= 1;
    }

    let mut i = n;
    while i > 1 {
        keys.swap(0, i - 1);
        down_heap(keys, 1, i - 1, comparer);
        i -= 1;
    }
}

/// `DownHeap` moves `d = keys[i - 1]` down by shifting children up; swapping it along the path
/// leaves the same arrangement.
fn down_heap<T>(keys: &mut [T], mut i: usize, n: usize, comparer: &mut impl FnMut(&T, &T) -> i32) {
    while i <= n >> 1 {
        let mut child = 2 * i;
        if child < n && comparer(&keys[child - 1], &keys[child]) < 0 {
            child += 1;
        }

        if comparer(&keys[i - 1], &keys[child - 1]) >= 0 {
            break;
        }

        keys.swap(i - 1, child - 1);
        i = child;
    }
}

/// `InsertionSort` shifts larger elements right past `t = keys[i + 1]`; swapping `t` down step by
/// step leaves the same arrangement.
fn insertion_sort<T>(keys: &mut [T], comparer: &mut impl FnMut(&T, &T) -> i32) {
    for i in 0..keys.len().saturating_sub(1) {
        let mut j = i + 1;
        while j > 0 && comparer(&keys[j], &keys[j - 1]) < 0 {
            keys.swap(j, j - 1);
            j -= 1;
        }
    }
}

/// `a.CompareTo(b)` for an integer.
#[must_use]
pub fn compare_to<T: Ord>(a: T, b: T) -> i32 {
    match a.cmp(&b) {
        Ordering::Less => -1,
        Ordering::Equal => 0,
        Ordering::Greater => 1,
    }
}

/// `float.CompareTo(float)`: NaN sorts below every number and equals NaN; `-0.0 == 0.0`.
#[must_use]
pub fn float_compare_to(a: f32, b: f32) -> i32 {
    double_compare_to(f64::from(a), f64::from(b))
}

/// `double.CompareTo(double)`: NaN sorts below every number and equals NaN; `-0.0 == 0.0`.
#[must_use]
pub fn double_compare_to(a: f64, b: f64) -> i32 {
    if a < b {
        -1
    } else if a > b {
        1
    } else if a == b {
        0
    } else if a.is_nan() {
        if b.is_nan() {
            0
        } else {
            -1
        }
    } else {
        1
    }
}
