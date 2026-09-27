//! Files that hold a secret (the session's tokens, this Mac's exec credentials, the file vault):
//! readable by this user only, from the moment they exist.

use std::path::Path;

/// Write `bytes` to `path`, readable and writable by this user alone.
///
/// `OpenOptions::mode` applies only when the open creates the file, so a file that was already
/// there keeps whatever mode it had: a session file left `0644` by an older build, or by
/// anything else, would stay readable by every account on the Mac with fresh tokens in it. The
/// mode is set on the open file as well, before anything is written, and a failure to set it is
/// an error rather than a secret written anyway.
pub fn write_private(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(path)?;
        file.set_permissions(std::fs::Permissions::from_mode(0o600))?;
        std::io::Write::write_all(&mut file, bytes)?;
        file.sync_all()
    }
    #[cfg(not(unix))]
    {
        std::fs::write(path, bytes)
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::write_private;
    use std::os::unix::fs::PermissionsExt;

    fn mode(path: &std::path::Path) -> u32 {
        std::fs::metadata(path).unwrap().permissions().mode() & 0o777
    }

    /// A new file is private from the start, and one that was already there, readable by
    /// everyone, is made private before the new contents go in.
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
    }
}
