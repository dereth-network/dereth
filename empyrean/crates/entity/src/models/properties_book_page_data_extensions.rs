// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Models/PropertiesBookPageDataExtensions.cs
//! `PropertiesBookPageDataExtensions`: helpers over a biota's `IList<PropertiesBookPageData>`,
//! which may be `null` (`None`). ACE's `ReaderWriterLockSlim` parameter is dropped (see
//! [`crate::models`]).

use crate::models::properties_book_page_data::PropertiesBookPageData;

/// `IList[index]` / `RemoveAt(index)` with a negative index: .NET throws.
fn negative_index(index: i32) -> ! {
    panic!("ArgumentOutOfRangeException: Index was out of range. Must be non-negative and less than the size of the collection. (index {index})")
}

/// `Count`, or 0 for a null list.
// ACE: PropertiesBookPageDataExtensions.GetPageCount
#[must_use]
pub fn get_page_count(value: Option<&Vec<PropertiesBookPageData>>) -> i32 {
    let Some(value) = value else { return 0 };

    i32::try_from(value.len()).unwrap_or(i32::MAX)
}

/// A new list with the same pages, or `None` for a null list. ACE's copy shares the page
/// objects; this one copies them.
// ACE: PropertiesBookPageDataExtensions.Clone
#[must_use]
pub fn clone(value: Option<&Vec<PropertiesBookPageData>>) -> Option<Vec<PropertiesBookPageData>> {
    let value = value?;

    Some(value.clone())
}

/// The page at `index`, or `None` for a null list or an index at or past the end.
///
/// # Panics
/// On a negative index, where ACE's indexer throws.
// ACE: PropertiesBookPageDataExtensions.GetPage
#[must_use]
pub fn get_page(
    value: Option<&Vec<PropertiesBookPageData>>,
    index: i32,
) -> Option<&PropertiesBookPageData> {
    let value = value?;

    if i64::try_from(value.len()).unwrap_or(i64::MAX) <= i64::from(index) {
        return None;
    }

    match usize::try_from(index) {
        Ok(i) => Some(&value[i]),
        Err(_) => negative_index(index),
    }
}

/// Appends `page` and returns its index (ACE's `out int index`). The list must exist, as in ACE.
// ACE: PropertiesBookPageDataExtensions.AddPage
pub fn add_page(value: &mut Vec<PropertiesBookPageData>, page: PropertiesBookPageData) -> i32 {
    value.push(page);
    i32::try_from(value.len()).unwrap_or(i32::MAX) - 1
}

/// Removes the page at `index`; false for a null list or an index at or past the end.
///
/// # Panics
/// On a negative index, where ACE's `RemoveAt` throws.
// ACE: PropertiesBookPageDataExtensions.RemovePage
pub fn remove_page(value: Option<&mut Vec<PropertiesBookPageData>>, index: i32) -> bool {
    let Some(value) = value else { return false };

    if i64::try_from(value.len()).unwrap_or(i64::MAX) <= i64::from(index) {
        return false;
    }

    match usize::try_from(index) {
        Ok(i) => {
            value.remove(i);
            true
        }
        Err(_) => negative_index(index),
    }
}
