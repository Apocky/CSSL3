//! Encrypted local storage for the refresh token and conversation references.
//!
//! Ported from the Android client's `SecureStore.java`. Android binds the
//! ciphertext to a Keystore key; Windows has no equivalent app-scoped keystore,
//! so this uses DPAPI under the signed-in Windows user with the record name as
//! additional entropy. That binds a file to both the user account and the
//! record it claims to be, so a copied or renamed file will not decrypt.
//!
//! What is stored is deliberately small: a refresh token, the account id, and
//! conversation identifiers. No transcript and no access token ever lands here.

use std::fs;
use std::path::{Path, PathBuf};

use crate::protocol::{ProtocolError, Result};

const NAMESPACE: &str = "apocrypha.desktop.session.v1";
const MAX_RECORD_BYTES: u64 = 131_072;

fn reject(message: &str) -> ProtocolError {
    ProtocolError::Rejected(message.to_string())
}

pub struct SecureStore {
    root: PathBuf,
}

impl SecureStore {
    /// Resolves the folder without creating it.
    ///
    /// Creation is deferred to the first write, so opening the application and
    /// never signing in leaves nothing at all behind on the computer.
    pub fn new() -> Result<Self> {
        let base = std::env::var("LOCALAPPDATA")
            .map_err(|_| reject("This computer has no local application data folder."))?;
        Ok(Self { root: Path::new(&base).join("Apocky").join("Apocrypha") })
    }

    #[cfg(test)]
    pub fn at(root: PathBuf) -> Result<Self> {
        Ok(Self { root })
    }

    fn path(&self, name: &str) -> Result<PathBuf> {
        // Record names include an account id after a colon, which Windows will
        // not accept in a filename; anything outside this set is a caller bug.
        let safe: String = name
            .chars()
            .map(|ch| match ch {
                'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' => ch,
                ':' | '.' => '_',
                _ => '\u{0}',
            })
            .collect();
        if safe.is_empty() || safe.contains('\u{0}') || safe.len() > 128 {
            return Err(reject("Your secure session could not be addressed."));
        }
        Ok(self.root.join(format!("{safe}.bin")))
    }

    fn entropy(name: &str) -> Vec<u8> {
        format!("{NAMESPACE}:{name}").into_bytes()
    }

    pub fn put(&self, name: &str, data: &serde_json::Value) -> Result<()> {
        let path = self.path(name)?;
        fs::create_dir_all(&self.root)
            .map_err(|_| reject("Your secure session folder could not be created."))?;
        let plaintext = serde_json::to_vec(data)
            .map_err(|_| reject("Your secure session could not be saved."))?;
        let sealed = protect(&plaintext, &Self::entropy(name))?;
        fs::write(&path, sealed).map_err(|_| reject("Your secure session could not be saved."))
    }

    pub fn get(&self, name: &str) -> Result<Option<serde_json::Value>> {
        let path = self.path(name)?;
        let metadata = match fs::metadata(&path) {
            Ok(metadata) => metadata,
            Err(_) => return Ok(None),
        };
        if metadata.len() > MAX_RECORD_BYTES {
            return Err(reject("Your saved session is damaged. Sign out and reconnect."));
        }
        let sealed = fs::read(&path)
            .map_err(|_| reject("Your saved session is damaged. Sign out and reconnect."))?;
        let plaintext = unprotect(&sealed, &Self::entropy(name))?;
        let value = serde_json::from_slice(&plaintext)
            .map_err(|_| reject("Your saved session is damaged. Sign out and reconnect."))?;
        Ok(Some(value))
    }

    /// Removes every record this application owns.
    pub fn clear(&self) -> Result<()> {
        let entries = match fs::read_dir(&self.root) {
            Ok(entries) => entries,
            Err(_) => return Ok(()),
        };
        let mut failed = false;
        for entry in entries.flatten() {
            if entry.path().extension().and_then(|value| value.to_str()) == Some("bin") {
                failed |= fs::remove_file(entry.path()).is_err();
            }
        }
        if failed {
            return Err(reject(
                "Sign-out could not clear secure storage. Remove the Apocrypha folder in %LOCALAPPDATA%\\Apocky before sharing this computer.",
            ));
        }
        Ok(())
    }
}

#[cfg(windows)]
fn protect(plaintext: &[u8], entropy: &[u8]) -> Result<Vec<u8>> {
    windows_dpapi::run(plaintext, entropy, true)
}

#[cfg(windows)]
fn unprotect(sealed: &[u8], entropy: &[u8]) -> Result<Vec<u8>> {
    windows_dpapi::run(sealed, entropy, false)
}

#[cfg(windows)]
mod windows_dpapi {
    use super::{reject, Result};
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{LocalFree, HLOCAL};
    use windows::Win32::Security::Cryptography::{
        CryptProtectData, CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
    };

    fn blob(bytes: &[u8]) -> CRYPT_INTEGER_BLOB {
        CRYPT_INTEGER_BLOB {
            cbData: bytes.len() as u32,
            pbData: bytes.as_ptr() as *mut u8,
        }
    }

    pub fn run(input: &[u8], entropy: &[u8], seal: bool) -> Result<Vec<u8>> {
        let source = blob(input);
        let salt = blob(entropy);
        let mut output = CRYPT_INTEGER_BLOB::default();
        let entropy_ptr: *const CRYPT_INTEGER_BLOB = &salt;
        let called = unsafe {
            if seal {
                CryptProtectData(
                    &source,
                    PCWSTR::null(),
                    Some(entropy_ptr),
                    None,
                    None,
                    CRYPTPROTECT_UI_FORBIDDEN,
                    &mut output,
                )
            } else {
                CryptUnprotectData(
                    &source,
                    None,
                    Some(entropy_ptr),
                    None,
                    None,
                    CRYPTPROTECT_UI_FORBIDDEN,
                    &mut output,
                )
            }
        };
        if called.is_err() || output.pbData.is_null() {
            return Err(reject(if seal {
                "Your secure session could not be saved."
            } else {
                "Your saved session is damaged. Sign out and reconnect."
            }));
        }
        let result = unsafe { std::slice::from_raw_parts(output.pbData, output.cbData as usize) }.to_vec();
        unsafe {
            std::ptr::write_bytes(output.pbData, 0, output.cbData as usize);
            let _ = LocalFree(HLOCAL(output.pbData as *mut _));
        }
        Ok(result)
    }
}

#[cfg(not(windows))]
fn protect(_plaintext: &[u8], _entropy: &[u8]) -> Result<Vec<u8>> {
    Err(reject("Secure local storage is available on Windows only."))
}

#[cfg(not(windows))]
fn unprotect(_sealed: &[u8], _entropy: &[u8]) -> Result<Vec<u8>> {
    Err(reject("Secure local storage is available on Windows only."))
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    fn temp_store(tag: &str) -> (SecureStore, PathBuf) {
        let root = std::env::temp_dir().join(format!("apocrypha-store-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        (SecureStore::at(root.clone()).unwrap(), root)
    }

    #[test]
    fn opening_the_store_writes_nothing_until_something_is_saved() {
        let (store, root) = temp_store("lazy");
        let _ = fs::remove_dir_all(&root);
        assert!(store.get("auth").unwrap().is_none());
        assert!(!root.exists(), "a person who never signs in leaves nothing behind");
        store.put("auth", &serde_json::json!({ "refresh_token": "value" })).unwrap();
        assert!(root.exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn records_round_trip_through_the_encrypted_file() {
        let (store, root) = temp_store("roundtrip");
        assert!(store.get("auth").unwrap().is_none());
        let saved = serde_json::json!({ "refresh_token": "value", "user_id": "abc" });
        store.put("auth", &saved).unwrap();
        assert_eq!(store.get("auth").unwrap().unwrap(), saved);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn the_refresh_token_is_not_readable_on_disk() {
        let (store, root) = temp_store("opaque");
        store.put("auth", &serde_json::json!({ "refresh_token": "SECRET-TOKEN-MATERIAL" })).unwrap();
        let raw = fs::read(root.join("auth.bin")).unwrap();
        let found = raw
            .windows("SECRET-TOKEN-MATERIAL".len())
            .any(|window| window == b"SECRET-TOKEN-MATERIAL");
        assert!(!found, "the stored file must not contain the token in the clear");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn a_record_will_not_decrypt_under_another_name() {
        let (store, root) = temp_store("rebind");
        store.put("auth", &serde_json::json!({ "refresh_token": "value" })).unwrap();
        fs::copy(root.join("auth.bin"), root.join("refs_other.bin")).unwrap();
        assert!(store.get("refs:other").is_err(), "entropy must bind a record to its name");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn account_scoped_names_become_valid_filenames() {
        let (store, root) = temp_store("names");
        let name = "refs:0f9c1d2e-3a4b-4c6d-8e8f-90a1b2c3d4e5";
        store.put(name, &serde_json::json!({ "ids": [] })).unwrap();
        assert!(store.get(name).unwrap().is_some());
        assert!(store.path(name).unwrap().to_string_lossy().contains("refs_0f9c1d2e"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn clearing_removes_every_owned_record() {
        let (store, root) = temp_store("clear");
        store.put("auth", &serde_json::json!({ "refresh_token": "value" })).unwrap();
        store.put("refs:a", &serde_json::json!({ "ids": [] })).unwrap();
        store.clear().unwrap();
        assert!(store.get("auth").unwrap().is_none());
        assert!(store.get("refs:a").unwrap().is_none());
        let _ = fs::remove_dir_all(root);
    }
}
