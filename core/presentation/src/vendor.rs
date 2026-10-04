//! Vendor quantity, price and purse lines.

#[must_use]
pub fn transaction_line(verb: &str, items: i32, value: i32) -> String {
    let noun = if items == 1 { "item" } else { "items" };
    format!(
        "{verb} {items} {noun} worth {}p",
        crate::appraisal::insert_commas(value)
    )
}

#[must_use]
pub fn purse_line(total_value: i32) -> String {
    format!("You have {}p", crate::appraisal::insert_commas(total_value))
}

#[must_use]
pub fn item_name_line(name: &str, plural: Option<&str>, split_size: u32) -> String {
    if split_size > 1 {
        format!("{split_size} {}", plural.unwrap_or(name))
    } else {
        name.to_string()
    }
}

#[must_use]
pub fn item_cost_line(price: i32, total_value: i32, split_size: u32) -> String {
    let verb = if split_size > 1 { "cost" } else { "costs" };
    format!(
        "{verb} {}p (you have {}p)",
        crate::appraisal::insert_commas(price),
        crate::appraisal::insert_commas(total_value)
    )
}
