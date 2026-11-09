use sha2::{Digest, Sha256};

pub fn verify(bytes: &[u8], expected: &str) -> Result<(), String> {
    let actual: String = Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    if actual == expected {
        Ok(())
    } else {
        Err(format!(
            "boot ROM checksum mismatch: expected {expected}, got {actual}"
        ))
    }
}

/// Prefer the original host, accepting either source only with exact bytes.
/// The caller publishes the cache only after this function succeeds.
pub fn download_verified(
    primary: &str,
    backup: &str,
    expected: &str,
    mut fetch: impl FnMut(&str) -> Result<Vec<u8>, String>,
) -> Result<Vec<u8>, String> {
    let mut errors = Vec::new();
    for url in [primary, backup] {
        match fetch(url).and_then(|bytes| {
            verify(&bytes, expected)?;
            Ok(bytes)
        }) {
            Ok(bytes) => return Ok(bytes),
            Err(error) => errors.push(format!("{url}: {error}")),
        }
    }
    Err(errors.join("; "))
}
