//! Password backend. UI never calls [`SecretStore::get`] to display.
//! The login card and Settings→Logins ask [`SecretStore::contains`] only; the
//! password is read once, after Touch ID, on its way to the computer.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use super::{KEYCHAIN_SERVICE, StoreError, VAULT_FILE};

pub trait SecretStore: Send + Sync {
    fn set(&self, id: &str, secret: &str) -> Result<(), StoreError>;
    /// After Touch ID, on the way to the computer, and in tests. Never decrypt-for-display.
    fn get(&self, id: &str) -> Result<Option<String>, StoreError>;
    fn delete(&self, id: &str) -> Result<(), StoreError>;
    fn contains(&self, id: &str) -> bool;
}

pub struct MemorySecrets {
    inner: Mutex<HashMap<String, String>>,
}

impl MemorySecrets {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(HashMap::new()),
        }
    }
}

impl SecretStore for MemorySecrets {
    fn set(&self, id: &str, secret: &str) -> Result<(), StoreError> {
        self.inner
            .lock()
            .map_err(|err| StoreError::Secret(err.to_string()))?
            .insert(id.to_string(), secret.to_string());
        Ok(())
    }

    fn get(&self, id: &str) -> Result<Option<String>, StoreError> {
        Ok(self
            .inner
            .lock()
            .map_err(|err| StoreError::Secret(err.to_string()))?
            .get(id)
            .cloned())
    }

    fn delete(&self, id: &str) -> Result<(), StoreError> {
        self.inner
            .lock()
            .map_err(|err| StoreError::Secret(err.to_string()))?
            .remove(id);
        Ok(())
    }

    fn contains(&self, id: &str) -> bool {
        self.inner
            .lock()
            .map(|map| map.contains_key(id))
            .unwrap_or(false)
    }
}

/// JSON map `id → password` with mode 0600. Used when OS Keychain is missing.
#[cfg_attr(target_os = "macos", allow(dead_code))]
pub struct FileVault {
    path: PathBuf,
    lock: Mutex<()>,
}

#[cfg_attr(target_os = "macos", allow(dead_code))]
impl FileVault {
    pub fn new(data_dir: &Path) -> Self {
        Self {
            path: data_dir.join(VAULT_FILE),
            lock: Mutex::new(()),
        }
    }

    fn read_map(&self) -> Result<HashMap<String, String>, StoreError> {
        if !self.path.exists() {
            return Ok(HashMap::new());
        }
        let bytes = fs::read(&self.path).map_err(|err| StoreError::Io(err.to_string()))?;
        if bytes.is_empty() {
            return Ok(HashMap::new());
        }
        serde_json::from_slice(&bytes).map_err(|err| StoreError::Io(err.to_string()))
    }

    fn write_map(&self, map: &HashMap<String, String>) -> Result<(), StoreError> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).map_err(|err| StoreError::Io(err.to_string()))?;
        }
        let bytes = serde_json::to_vec(map).map_err(|err| StoreError::Io(err.to_string()))?;
        let tmp = self.path.with_extension("vault.tmp");
        fs::write(&tmp, bytes).map_err(|err| StoreError::Io(err.to_string()))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(&tmp, fs::Permissions::from_mode(0o600));
        }
        fs::rename(&tmp, &self.path).map_err(|err| StoreError::Io(err.to_string()))?;
        Ok(())
    }
}

#[cfg_attr(target_os = "macos", allow(dead_code))]
impl SecretStore for FileVault {
    fn set(&self, id: &str, secret: &str) -> Result<(), StoreError> {
        let _guard = self
            .lock
            .lock()
            .map_err(|err| StoreError::Secret(err.to_string()))?;
        let mut map = self.read_map()?;
        map.insert(id.to_string(), secret.to_string());
        self.write_map(&map)
    }

    fn get(&self, id: &str) -> Result<Option<String>, StoreError> {
        let _guard = self
            .lock
            .lock()
            .map_err(|err| StoreError::Secret(err.to_string()))?;
        Ok(self.read_map()?.get(id).cloned())
    }

    fn delete(&self, id: &str) -> Result<(), StoreError> {
        let _guard = self
            .lock
            .lock()
            .map_err(|err| StoreError::Secret(err.to_string()))?;
        let mut map = self.read_map()?;
        map.remove(id);
        self.write_map(&map)
    }

    fn contains(&self, id: &str) -> bool {
        self.get(id).ok().flatten().is_some()
    }
}

#[cfg(target_os = "macos")]
pub struct KeychainSecrets;

#[cfg(target_os = "macos")]
impl SecretStore for KeychainSecrets {
    fn set(&self, id: &str, secret: &str) -> Result<(), StoreError> {
        security_framework::passwords::set_generic_password(KEYCHAIN_SERVICE, id, secret.as_bytes())
            .map_err(|err| StoreError::Secret(err.to_string()))
    }

    fn get(&self, id: &str) -> Result<Option<String>, StoreError> {
        match security_framework::passwords::generic_password(
            security_framework::passwords::PasswordOptions::new_generic_password(
                KEYCHAIN_SERVICE,
                id,
            ),
        ) {
            Ok(bytes) => String::from_utf8(bytes)
                .map(Some)
                .map_err(|err| StoreError::Secret(err.to_string())),
            Err(err) if is_not_found(err) => Ok(None),
            Err(err) => Err(StoreError::Secret(err.to_string())),
        }
    }

    fn delete(&self, id: &str) -> Result<(), StoreError> {
        match security_framework::passwords::delete_generic_password(KEYCHAIN_SERVICE, id) {
            Ok(()) => Ok(()),
            Err(err) if is_not_found(err) => Ok(()),
            Err(err) => Err(StoreError::Secret(err.to_string())),
        }
    }

    /// Whether the item is there, asked by its attributes alone. Reading a secret is what
    /// makes the keychain ask the person to unlock it; a search that loads no data does
    /// not, so the app can know which rows it holds without putting a sheet up for each.
    fn contains(&self, id: &str) -> bool {
        use security_framework::item::{ItemClass, ItemSearchOptions, Limit};
        ItemSearchOptions::new()
            .class(ItemClass::generic_password())
            .service(KEYCHAIN_SERVICE)
            .account(id)
            .load_attributes(true)
            .limit(Limit::Max(1))
            .search()
            .is_ok_and(|found| !found.is_empty())
    }
}

#[cfg(target_os = "macos")]
fn is_not_found(err: security_framework::base::Error) -> bool {
    // errSecItemNotFound
    err.code() == -25300
}

pub fn open_secrets(data_dir: &Path) -> std::sync::Arc<dyn SecretStore> {
    #[cfg(target_os = "macos")]
    {
        let _ = data_dir;
        std::sync::Arc::new(KeychainSecrets)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = KEYCHAIN_SERVICE;
        std::sync::Arc::new(FileVault::new(data_dir))
    }
}
