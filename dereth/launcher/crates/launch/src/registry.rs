//! The signed registry snapshot, and which source is trusted for what.
//!
//! The live `/v1/servers` API is trusted through TLS for status and population. Fields that change
//! what the launcher *does* (where it connects, which dats and clients it expects) are taken from a
//! signed snapshot instead: `worlds.json` plus `worlds.json.sig`, an Ed25519 signature over the
//! exact bytes of the JSON, hex-encoded, by a key whose public half is compiled in. A snapshot that
//! is unsigned or fails verification is ignored and the last good cache is used.
//!
//! No signing key has been minted yet, so [`PINNED_KEYS`] is empty and every snapshot is refused:
//! the launcher behaves exactly as it did before snapshots existed, trusting the live API for
//! everything. Adding the first key turns the signed path on.

use std::path::{Path, PathBuf};

use ed25519_dalek::{Signature, VerifyingKey};

use crate::world::World;

/// The keys a snapshot may be signed with: the current one, and room for its successor during a
/// rotation.
pub const PINNED_KEYS: &[[u8; 32]] = &[];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SignatureError {
    NoKeys,
    Malformed,
    Invalid,
}

impl core::fmt::Display for SignatureError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            SignatureError::NoKeys => write!(f, "no registry key is pinned"),
            SignatureError::Malformed => write!(f, "the signature is not a signature"),
            SignatureError::Invalid => write!(f, "the signature does not match"),
        }
    }
}

fn unhex(s: &str) -> Option<Vec<u8>> {
    let s = s.trim();
    if !s.len().is_multiple_of(2) {
        return None;
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(s.get(i..i + 2)?, 16).ok())
        .collect()
}

/// Check `body` against a hex signature, with any of `keys`.
pub fn verify(body: &[u8], signature_hex: &str, keys: &[[u8; 32]]) -> Result<(), SignatureError> {
    if keys.is_empty() {
        return Err(SignatureError::NoKeys);
    }
    let bytes: [u8; 64] = unhex(signature_hex)
        .and_then(|b| b.try_into().ok())
        .ok_or(SignatureError::Malformed)?;
    let sig = Signature::from_bytes(&bytes);
    for k in keys {
        if let Ok(key) = VerifyingKey::from_bytes(k) {
            if key.verify_strict(body, &sig).is_ok() {
                return Ok(());
            }
        }
    }
    Err(SignatureError::Invalid)
}

/// Combine the signed snapshot with the live list.
///
/// A world in both takes what it does from the snapshot and how it is doing from the live list. A
/// world only in the live list keeps its address (as the launcher always has) but none of its
/// claims about dats or clients, which then fall back to what the check assumes when not told. A
/// world only in the snapshot is listed with its status unknown.
pub fn merge(signed: &[World], live: &[World]) -> Vec<World> {
    let mut out = Vec::with_capacity(live.len().max(signed.len()));
    for l in live {
        match signed.iter().find(|s| s.slug == l.slug) {
            Some(s) => {
                let mut w = s.clone();
                w.state = l.state;
                w.players = l.players;
                out.push(w);
            }
            None => {
                let mut w = l.clone();
                w.dats = Default::default();
                w.accepted_clients.clear();
                w.preferred_client = None;
                out.push(w);
            }
        }
    }
    for s in signed {
        if !live.iter().any(|l| l.slug == s.slug) {
            out.push(s.clone());
        }
    }
    out
}

/// The on-disk cache: `<state>/registry/worlds.json`, its signature, and the ETag it came with.
#[derive(Debug, Clone)]
pub struct Cache {
    dir: PathBuf,
}

impl Cache {
    pub fn new(state_dir: &Path) -> Self {
        Self {
            dir: state_dir.join("registry"),
        }
    }

    /// The cached snapshot, if one verifies.
    pub fn load(&self, keys: &[[u8; 32]]) -> Option<Vec<u8>> {
        let body = std::fs::read(self.dir.join("worlds.json")).ok()?;
        let sig = std::fs::read_to_string(self.dir.join("worlds.json.sig")).ok()?;
        verify(&body, &sig, keys).ok()?;
        Some(body)
    }

    pub fn etag(&self) -> Option<String> {
        std::fs::read_to_string(self.dir.join("etag"))
            .ok()
            .filter(|s| !s.is_empty())
    }

    /// Keep a snapshot, only if it verifies: a bad download must never replace a good cache.
    pub fn store(
        &self,
        body: &[u8],
        sig: &str,
        etag: Option<&str>,
        keys: &[[u8; 32]],
    ) -> Result<(), SignatureError> {
        verify(body, sig, keys)?;
        let _ = std::fs::create_dir_all(&self.dir);
        let _ = std::fs::write(self.dir.join("worlds.json"), body);
        let _ = std::fs::write(self.dir.join("worlds.json.sig"), sig);
        let _ = std::fs::write(self.dir.join("etag"), etag.unwrap_or(""));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::datset::tests::tmp;
    use crate::datset::Iterations;
    use crate::world::WorldState;
    use ed25519_dalek::{Signer, SigningKey};

    fn key() -> SigningKey {
        SigningKey::from_bytes(&[7u8; 32])
    }

    fn sign(body: &[u8]) -> String {
        crate::install::hex(&key().sign(body).to_bytes())
    }

    #[test]
    fn a_signed_snapshot_verifies_and_a_tampered_one_does_not() {
        let keys = [key().verifying_key().to_bytes()];
        let body = br#"[{"slug":"eulmore"}]"#;
        let sig = sign(body);
        assert_eq!(verify(body, &sig, &keys), Ok(()));
        assert_eq!(
            verify(br#"[{"slug":"evil"}]"#, &sig, &keys),
            Err(SignatureError::Invalid)
        );
        assert_eq!(verify(body, "zz", &keys), Err(SignatureError::Malformed));
        assert_eq!(verify(body, &sig, &[]), Err(SignatureError::NoKeys));
    }

    #[test]
    fn with_no_key_pinned_every_snapshot_is_refused() {
        assert!(
            PINNED_KEYS.is_empty(),
            "when a key is minted, this test and the module doc change with it"
        );
        assert_eq!(
            verify(b"x", &sign(b"x"), PINNED_KEYS),
            Err(SignatureError::NoKeys)
        );
    }

    #[test]
    fn behaviour_comes_from_the_snapshot_and_status_from_the_live_list() {
        let mut signed = World::new("eulmore", "Eulmore");
        signed.dats.expected = Some(Iterations::END_OF_RETAIL);
        signed.accepted_clients = vec!["dereth".into()];
        let mut live = World::new("eulmore", "Eulmore");
        live.state = WorldState::Online;
        live.players = Some(12);
        live.accepted_clients = vec!["anything".into()];
        let mut stray = World::new("stray", "Stray");
        stray.dats.patches_over_wire = Some(true);

        let merged = merge(&[signed], &[live, stray]);
        assert_eq!(
            merged[0].accepted_clients,
            ["dereth"],
            "the live list cannot change what is accepted"
        );
        assert_eq!(
            (merged[0].state, merged[0].players),
            (WorldState::Online, Some(12))
        );
        assert_eq!(
            merged[1].dats.patches_over_wire, None,
            "an unsigned claim is dropped"
        );
    }

    #[test]
    fn the_cache_keeps_only_what_verifies() {
        let d = tmp("registry");
        let keys = [key().verifying_key().to_bytes()];
        let c = Cache::new(&d);
        assert!(c.store(b"[]", "00", None, &keys).is_err());
        assert_eq!(c.load(&keys), None);
        c.store(b"[]", &sign(b"[]"), Some("\"abc\""), &keys)
            .unwrap();
        assert_eq!(c.load(&keys).as_deref(), Some(&b"[]"[..]));
        assert_eq!(c.etag().as_deref(), Some("\"abc\""));
        let _ = std::fs::remove_dir_all(&d);
    }
}
