//! The system's random source, for what must not be guessed.

/// 64 bits from the system's random source, or why there are none. Never anything else: login
/// cookies and checksum seeds are read from here (`yalain_net::ServerNet::new`), and nothing stands
/// in for it when it fails.
///
/// # Errors
/// The source failed.
pub fn os_u64() -> Result<u64, String> {
    let mut bytes = [0u8; 8];
    getrandom::fill(&mut bytes).map_err(|e| format!("the system's random source: {e}"))?;
    Ok(u64::from_le_bytes(bytes))
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (a thin call of the system's source: what it gives is the system's to say)
    use super::*;

    #[test]
    fn the_source_gives_a_different_number_each_time() {
        let a = os_u64().expect("the source reads");
        let b = os_u64().expect("the source reads");
        assert_ne!(a, b, "2^-64 by chance");
    }
}
