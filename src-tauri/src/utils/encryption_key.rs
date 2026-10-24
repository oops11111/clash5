use anyhow::{Result, anyhow};

/// Where the application-wide encryption key is kept. Failures are returned, never replaced by a
/// fallback, so encrypted data is not silently written under a different key.
pub trait KeyStore {
    fn load(&self) -> Result<Option<Vec<u8>>>;
    fn save(&self, key: &[u8]) -> Result<()>;
}

/// Returns the key to use, and whether the legacy key file may be removed.
///
/// A key in the store is authoritative. A legacy file is moved into the store only after the store
/// reads the same bytes back, so an interrupted migration leaves the file in place.
pub fn resolve_key(store: &impl KeyStore, legacy: Option<Vec<u8>>) -> Result<(Vec<u8>, bool)> {
    if let Some(key) = store.load()? {
        let legacy_matches = legacy.as_deref() == Some(key.as_slice());
        return Ok((key, legacy_matches));
    }
    let key = match legacy {
        Some(key) => key,
        None => {
            let mut key = vec![0u8; 32];
            getrandom::fill(&mut key).map_err(|error| anyhow!("OS random source is unavailable: {error}"))?;
            key
        }
    };
    store.save(&key)?;
    if store.load()?.as_deref() != Some(key.as_slice()) {
        return Err(anyhow!("encryption key could not be verified in the credential store"));
    }
    Ok((key, true))
}

#[cfg(not(target_os = "linux"))]
pub struct OsKeyStore;

#[cfg(not(target_os = "linux"))]
impl KeyStore for OsKeyStore {
    fn load(&self) -> Result<Option<Vec<u8>>> {
        match keyring_entry()?.get_secret() {
            Ok(bytes) => Ok(Some(bytes)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(error) => Err(anyhow!("credential store read failed: {error}")),
        }
    }

    fn save(&self, key: &[u8]) -> Result<()> {
        keyring_entry()?
            .set_secret(key)
            .map_err(|error| anyhow!("credential store write failed: {error}"))
    }
}

#[cfg(not(target_os = "linux"))]
fn keyring_entry() -> Result<keyring::Entry> {
    keyring::Entry::new("io.github.oops11111.clash5", "encryption-key")
        .map_err(|error| anyhow!("credential store is unavailable: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    #[derive(Default)]
    struct MemoryStore {
        key: RefCell<Option<Vec<u8>>>,
        refuse_writes: bool,
    }

    impl KeyStore for MemoryStore {
        fn load(&self) -> Result<Option<Vec<u8>>> {
            Ok(self.key.borrow().clone())
        }

        fn save(&self, key: &[u8]) -> Result<()> {
            if self.refuse_writes {
                return Err(anyhow!("refused"));
            }
            *self.key.borrow_mut() = Some(key.to_vec());
            Ok(())
        }
    }

    #[test]
    fn a_stored_key_is_authoritative_and_only_matching_legacy_files_may_be_removed() -> Result<()> {
        let store = MemoryStore {
            key: RefCell::new(Some(vec![1; 32])),
            refuse_writes: false,
        };
        assert_eq!(resolve_key(&store, None)?, (vec![1; 32], false));
        assert_eq!(resolve_key(&store, Some(vec![1; 32]))?, (vec![1; 32], true));
        assert_eq!(resolve_key(&store, Some(vec![2; 32]))?, (vec![1; 32], false));
        Ok(())
    }

    #[test]
    fn a_legacy_file_is_moved_into_an_empty_store() -> Result<()> {
        let store = MemoryStore::default();
        assert_eq!(resolve_key(&store, Some(vec![7; 32]))?, (vec![7; 32], true));
        assert_eq!(store.key.borrow().clone(), Some(vec![7; 32]));
        Ok(())
    }

    #[test]
    fn a_failed_write_keeps_the_legacy_file_and_returns_an_error() {
        let store = MemoryStore {
            key: RefCell::new(None),
            refuse_writes: true,
        };
        assert!(resolve_key(&store, Some(vec![7; 32])).is_err());
    }

    #[test]
    fn a_new_key_is_generated_when_nothing_exists() -> Result<()> {
        let store = MemoryStore::default();
        let (key, legacy_removable) = resolve_key(&store, None)?;
        assert_eq!(key.len(), 32);
        assert!(legacy_removable);
        Ok(())
    }
}
