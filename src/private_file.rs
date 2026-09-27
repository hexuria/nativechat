//! Files that hold a secret (the session's tokens, this Mac's exec credentials, the file vault):
//! readable by this user only, from the moment their contents exist.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// Replace `path` with `bytes`, readable and writable by this user alone.
///
/// The bytes go into a new file, created `0600`, next to `path`, which is then renamed over it.
/// Never into the old file: permission is checked when a file is opened, so narrowing an
/// existing `0644` file and writing into it would still hand the new secret to anyone who had it
/// open already. And the old file is untouched until the rename, so any failure (creating,
/// narrowing, writing, syncing) leaves its bytes as they were and removes the new file.
pub fn write_private(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let staged = staged_path(path)?;
    let written = write_new(&staged, bytes).and_then(|()| std::fs::rename(&staged, path));
    if written.is_err() {
        let _ = std::fs::remove_file(&staged);
    }
    written
}

/// A name beside `path` that nothing else is using: same directory, so the rename stays on one
/// filesystem and is atomic.
fn staged_path(path: &Path) -> std::io::Result<PathBuf> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let name = path.file_name().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "a secret file needs a name",
        )
    })?;
    let n = NEXT.fetch_add(1, Ordering::Relaxed);
    Ok(path.with_file_name(format!(
        ".{}.{}-{n}.tmp",
        name.to_string_lossy(),
        std::process::id()
    )))
}

fn write_new(staged: &Path, bytes: &[u8]) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
        // create_new: a new inode, never one somebody else already holds open.
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(staged)?;
        // `mode` is filtered by the umask, which can only narrow it; say 0600 outright anyway.
        file.set_permissions(std::fs::Permissions::from_mode(0o600))?;
        std::io::Write::write_all(&mut file, bytes)?;
        file.sync_all()
    }
    // Not a shipping target: the umask decides this file's mode.
    #[cfg(not(unix))]
    {
        std::fs::write(staged, bytes)
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::write_private;
    use std::io::{Read, Seek, SeekFrom};
    use std::os::unix::fs::PermissionsExt;

    fn mode(path: &std::path::Path) -> u32 {
        std::fs::metadata(path).unwrap().permissions().mode() & 0o777
    }

    fn leftovers(dir: &std::path::Path) -> Vec<String> {
        std::fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
            .filter(|name| name.ends_with(".tmp"))
            .collect()
    }

    /// A new file is private, and so is one that was already there readable by everyone; either
    /// way the new contents are in it and nothing is left behind beside it.
    #[test]
    fn a_secret_file_is_private_whether_it_is_new_or_was_already_there() {
        let dir = tempfile::tempdir().unwrap();
        let fresh = dir.path().join("session.json");
        write_private(&fresh, b"new").unwrap();
        assert_eq!(mode(&fresh), 0o600);
        assert_eq!(std::fs::read(&fresh).unwrap(), b"new");

        let old = dir.path().join("left-open.json");
        std::fs::write(&old, b"stale").unwrap();
        std::fs::set_permissions(&old, std::fs::Permissions::from_mode(0o644)).unwrap();
        write_private(&old, b"tokens").unwrap();
        assert_eq!(mode(&old), 0o600);
        assert_eq!(std::fs::read(&old).unwrap(), b"tokens");
        assert!(leftovers(dir.path()).is_empty());
    }

    /// Somebody who opened the old, world-readable file before the write keeps reading the old
    /// bytes: the new secret goes into a different file, which they never opened.
    #[test]
    fn a_reader_who_already_had_the_old_file_open_never_sees_the_new_secret() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("session.json");
        std::fs::write(&path, b"old tokens").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        let mut held = std::fs::File::open(&path).unwrap();

        write_private(&path, b"new tokens").unwrap();

        let mut seen = String::new();
        held.seek(SeekFrom::Start(0)).unwrap();
        held.read_to_string(&mut seen).unwrap();
        assert_eq!(seen, "old tokens");
        assert_eq!(std::fs::read(&path).unwrap(), b"new tokens");
    }

    /// A write that fails leaves the old secret exactly as it was, and no half-written file.
    #[test]
    fn a_failed_write_leaves_the_old_secret_intact() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("session.json");
        std::fs::write(&path, b"old tokens").unwrap();
        // A directory nobody may create in: the new file cannot be made.
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o500)).unwrap();
        let result = write_private(&path, b"new tokens");
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700)).unwrap();

        assert!(result.is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"old tokens");
        assert!(leftovers(dir.path()).is_empty());
    }
}
