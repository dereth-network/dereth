//! The launcher's update signatures and `latest.json`.
//!
//! The launcher's updater accepts a download only with a signature from the update-signing key
//! whose public half is `plugins.updater.pubkey` in the launcher's `tauri.conf.json`. A signature
//! is a minisign signature box, base64-encoded as a whole, and its trusted comment names the
//! file and the version (`timestamp:<secs>\tfile:<name>\tversion:<version>`), the form Tauri's own
//! signer writes; the updater refuses a signature whose version differs from the one `latest.json`
//! announces.
//!
//! The private key comes from the environment, as Tauri's signer takes it:
//! `TAURI_SIGNING_PRIVATE_KEY` (the key file's text, base64-encoded as Tauri's `signer generate`
//! prints it, or a path to that file) and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`. Without a key a
//! package is built unsigned and says so, and no `latest.json` is written for it.
//!
//! Every signature made is verified at once, the way the updater verifies it ([`verify`]),
//! against the public key the launcher carries: a key that is not the launcher's fails the
//! package instead of shipping an update no launcher accepts.

use std::path::Path;

use base64::Engine as _;

/// The environment variables Tauri's signer reads, read the same way here.
pub const KEY_VAR: &str = "TAURI_SIGNING_PRIVATE_KEY";
pub const PASSWORD_VAR: &str = "TAURI_SIGNING_PRIVATE_KEY_PASSWORD";

/// The untrusted comment Tauri's signer writes.
const UNTRUSTED_COMMENT: &str = "signature from tauri secret key";

fn b64() -> base64::engine::GeneralPurpose {
    base64::engine::general_purpose::STANDARD
}

/// The signing key's text: the variable's value decoded from base64, or the file it names (itself
/// either the key's text or its base64).
pub fn key_text(value: &str) -> Result<String, String> {
    let value = value.trim();
    let raw = if Path::new(value).is_file() {
        std::fs::read_to_string(value).map_err(|e| format!("{KEY_VAR} names {value}: {e}"))?
    } else {
        value.to_owned()
    };
    let raw = raw.trim();
    if raw.starts_with("untrusted comment:") {
        return Ok(raw.to_owned());
    }
    let decoded = b64()
        .decode(raw)
        .map_err(|e| format!("{KEY_VAR} is neither a key file nor its base64: {e}"))?;
    String::from_utf8(decoded)
        .map(|t| t.trim().to_owned())
        .map_err(|_| format!("{KEY_VAR} does not decode to a key file"))
}

/// A signing key, ready to sign.
pub struct Signer {
    key: minisign::SecretKey,
}

impl Signer {
    /// The key in `text` (a minisign secret key file), opened with `password` when it is
    /// encrypted.
    pub fn from_key_text(text: &str, password: &str) -> Result<Self, String> {
        let unencrypted = minisign::SecretKeyBox::from_string(text)
            .and_then(minisign::SecretKeyBox::into_unencrypted_secret_key);
        let key = match unencrypted {
            Ok(k) => k,
            Err(_) => minisign::SecretKeyBox::from_string(text)
                .and_then(|b| b.into_secret_key(Some(password.to_owned())))
                .map_err(|e| {
                    format!(
                        "the update-signing key could not be opened ({e}); check {PASSWORD_VAR}"
                    )
                })?,
        };
        Ok(Self { key })
    }

    /// The signer the environment names, or `None` when it names no key.
    pub fn from_env() -> Result<Option<Self>, String> {
        let Some(value) = std::env::var(KEY_VAR).ok().filter(|v| !v.trim().is_empty()) else {
            return Ok(None);
        };
        let password = std::env::var(PASSWORD_VAR).unwrap_or_default();
        Self::from_key_text(&key_text(&value)?, &password).map(Some)
    }

    /// The signature of `data`, published as `file`, for release `version`: the base64 of the
    /// whole signature box, which is what `latest.json` carries.
    pub fn sign(
        &self,
        data: &[u8],
        file: &str,
        version: &str,
        epoch: i64,
    ) -> Result<String, String> {
        let trusted = trusted_comment(epoch, file, version);
        let signature = minisign::sign(
            None,
            &self.key,
            std::io::Cursor::new(data),
            Some(&trusted),
            Some(UNTRUSTED_COMMENT),
        )
        .map_err(|e| format!("signing {file}: {e}"))?;
        Ok(b64().encode(signature.to_string()))
    }
}

/// The trusted comment of a signature: when, which file and which version.
pub fn trusted_comment(epoch: i64, file: &str, version: &str) -> String {
    format!("timestamp:{epoch}\tfile:{file}\tversion:{version}")
}

/// Verify `signature` (base64, as `latest.json` carries it) over `data` with `public_key` (base64,
/// as `tauri.conf.json` carries it), step for step as the launcher's updater does: decode both,
/// check the signature and its trusted comment, then require the version the comment names, if
/// any, to be `version`.
pub fn verify(data: &[u8], signature: &str, public_key: &str, version: &str) -> Result<(), String> {
    let text = |what: &str, v: &str| -> Result<String, String> {
        let bytes = b64()
            .decode(v.trim())
            .map_err(|e| format!("the {what} is not base64: {e}"))?;
        String::from_utf8(bytes).map_err(|_| format!("the {what} is not text"))
    };
    let key = minisign_verify::PublicKey::decode(&text("public key", public_key)?)
        .map_err(|e| format!("the public key: {e}"))?;
    let sig = minisign_verify::Signature::decode(&text("signature", signature)?)
        .map_err(|e| format!("the signature: {e}"))?;
    key.verify(data, &sig, true)
        .map_err(|e| format!("the signature does not verify: {e}"))?;
    match signed_version(sig.trusted_comment()) {
        Some(v) if v.trim_start_matches('v') != version.trim_start_matches('v') => {
            Err(format!("the signature is for version {v}, not {version}"))
        }
        _ => Ok(()),
    }
}

/// The version a trusted comment names.
pub fn signed_version(trusted_comment: &str) -> Option<&str> {
    trusted_comment
        .split('\t')
        .find_map(|field| field.strip_prefix("version:"))
}

/// The launcher's update-signing public key, `plugins.updater.pubkey` in its `tauri.conf.json`.
pub fn launcher_public_key(tauri_conf: &str) -> Result<String, String> {
    let conf: serde_json::Value =
        serde_json::from_str(tauri_conf).map_err(|e| format!("tauri.conf.json: {e}"))?;
    conf["plugins"]["updater"]["pubkey"]
        .as_str()
        .filter(|k| !k.trim().is_empty())
        .map(str::to_owned)
        .ok_or_else(|| "tauri.conf.json has no plugins.updater.pubkey".to_owned())
}

/// One platform's entry of `latest.json`: the file's download address and its signature.
pub fn platform_piece(platform: &str, url: &str, signature: &str) -> serde_json::Value {
    serde_json::json!({ platform: { "url": url, "signature": signature } })
}

/// `latest.json`: the version, its notes and date, and every platform piece merged. Two pieces
/// naming the same platform are refused.
pub fn merge_latest(
    version: &str,
    notes: &str,
    pub_date: &str,
    pieces: &[serde_json::Value],
) -> Result<serde_json::Value, String> {
    let mut platforms = serde_json::Map::new();
    for piece in pieces {
        let map = piece
            .as_object()
            .ok_or("a latest.json piece is not an object")?;
        for (platform, entry) in map {
            if entry["url"].as_str().is_none() || entry["signature"].as_str().is_none() {
                return Err(format!("the {platform} piece lacks a url or a signature"));
            }
            if platforms.insert(platform.clone(), entry.clone()).is_some() {
                return Err(format!("two pieces name the platform {platform}"));
            }
        }
    }
    if platforms.is_empty() {
        return Err("no platform pieces to merge".to_owned());
    }
    Ok(serde_json::json!({
        "version": version,
        "notes": notes,
        "pub_date": pub_date,
        "platforms": platforms,
    }))
}
