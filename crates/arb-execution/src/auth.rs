use std::path::{Path, PathBuf};
use std::str::FromStr;

use alloy::signers::local::PrivateKeySigner;
use arb_core::error::{ArbError, Result};
use polymarket_client_sdk::auth::state::Authenticated;
use polymarket_client_sdk::auth::{LocalSigner, Normal, Signer as _};
use polymarket_client_sdk::clob::types::SignatureType;
use polymarket_client_sdk::{POLYGON, clob};
use tracing::info;

/// Default location for the private key file, relative to workspace root.
const DEFAULT_KEY_FILE: &str = "secrets/key.txt";

/// Read a private key from a file, trimming whitespace.
///
/// Accepts either:
/// - A hex string with `0x` prefix (e.g. `0xac09...`)
/// - A raw hex string without prefix (e.g. `ac09...`)
///
/// The file should contain only the private key, one line.
pub fn read_private_key(path: &Path) -> Result<String> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| ArbError::Config(format!("Failed to read key file {}: {e}", path.display())))?;

    let key = content.trim().to_string();

    if key.is_empty() {
        return Err(ArbError::Config("Key file is empty".into()));
    }

    // Validate it looks like a hex private key
    let hex_part = key.strip_prefix("0x").unwrap_or(&key);
    if hex_part.len() != 64 || !hex_part.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(ArbError::Config(
            "Invalid private key: expected 64 hex characters (with optional 0x prefix)".into(),
        ));
    }

    Ok(key)
}

/// Resolve the key file path. Checks, in order:
/// 1. Explicit path if provided
/// 2. `POLYMARKET_KEY_FILE` env var
/// 3. `secrets/key.txt` in the current directory
pub fn resolve_key_path(explicit: Option<&Path>) -> PathBuf {
    if let Some(p) = explicit {
        return p.to_path_buf();
    }

    if let Ok(env_path) = std::env::var("POLYMARKET_KEY_FILE") {
        return PathBuf::from(env_path);
    }

    PathBuf::from(DEFAULT_KEY_FILE)
}

/// Create an authenticated CLOB client from a private key string.
///
/// Returns both the authenticated client AND the `LocalSigner` used during
/// authentication. The signer is needed later for `client.sign(&signer, order)`
/// when placing real orders.
///
/// This follows the same pattern as `polymarket-cli-main/src/auth.rs`:
/// 1. Parse key → `LocalSigner`
/// 2. Set chain to Polygon mainnet (137)
/// 3. Authenticate via EIP-712 signature exchange
pub async fn create_authenticated_client(
    private_key: &str,
) -> Result<(clob::Client<Authenticated<Normal>>, PrivateKeySigner)> {
    let signer = LocalSigner::from_str(private_key)
        .map_err(|e| ArbError::Config(format!("Invalid private key: {e}")))?
        .with_chain_id(Some(POLYGON));

    let address = signer.address();
    info!(address = %address, "Authenticating with Polymarket CLOB");

    let client = clob::Client::default()
        .authentication_builder(&signer)
        .signature_type(SignatureType::Eoa)
        .authenticate()
        .await
        .map_err(|e| ArbError::Execution(format!("CLOB authentication failed: {e}")))?;

    info!("Authenticated successfully");
    Ok((client, signer))
}

/// Convenience: read key from file and create authenticated client in one step.
///
/// Returns both the authenticated client and the `LocalSigner`.
pub async fn authenticate_from_key_file(
    key_path: Option<&Path>,
) -> Result<(clob::Client<Authenticated<Normal>>, PrivateKeySigner)> {
    let path = resolve_key_path(key_path);
    let key = read_private_key(&path)?;
    create_authenticated_client(&key).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn test_read_valid_key_with_prefix() {
        let dir = std::env::temp_dir().join("arb_test_key_prefix");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("key.txt");

        let mut f = std::fs::File::create(&path).unwrap();
        writeln!(f, "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80").unwrap();

        let key = read_private_key(&path).unwrap();
        assert!(key.starts_with("0x"));
        assert_eq!(key.len(), 66); // 0x + 64 hex chars

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_read_valid_key_without_prefix() {
        let dir = std::env::temp_dir().join("arb_test_key_no_prefix");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("key.txt");

        let mut f = std::fs::File::create(&path).unwrap();
        writeln!(f, "ac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80").unwrap();

        let key = read_private_key(&path).unwrap();
        assert_eq!(key.len(), 64);

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_read_empty_key_fails() {
        let dir = std::env::temp_dir().join("arb_test_key_empty");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("key.txt");

        std::fs::write(&path, "  \n").unwrap();

        let result = read_private_key(&path);
        assert!(result.is_err());

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_read_invalid_key_fails() {
        let dir = std::env::temp_dir().join("arb_test_key_invalid");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("key.txt");

        std::fs::write(&path, "not-a-valid-key").unwrap();

        let result = read_private_key(&path);
        assert!(result.is_err());

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_missing_file_fails() {
        let path = Path::new("/tmp/does_not_exist_arb_key.txt");
        let result = read_private_key(path);
        assert!(result.is_err());
    }

    #[test]
    fn test_resolve_key_path_explicit() {
        let p = resolve_key_path(Some(Path::new("/custom/key.txt")));
        assert_eq!(p, PathBuf::from("/custom/key.txt"));
    }

    #[test]
    fn test_resolve_key_path_default() {
        // Clear env var to test default.
        // SAFETY: Only called in single-threaded test context.
        unsafe { std::env::remove_var("POLYMARKET_KEY_FILE") };
        let p = resolve_key_path(None);
        assert_eq!(p, PathBuf::from("secrets/key.txt"));
    }
}
