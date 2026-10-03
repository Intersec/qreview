//! `server.json`: where the server of a repository listens.
//!
//! The commands of an agent run in another process, and need the port and
//! the token. The file holds both, and only its owner can read it, so
//! another local user still cannot reach the server.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

const NAME: &str = "server.json";

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Address {
    pub pid: u32,
    pub port: u16,
    pub token: String,
}

impl Address {
    /// What the browser opens. The token goes once, and the cookie after.
    pub fn url(&self) -> String {
        format!("http://127.0.0.1:{}/?t={}", self.port, self.token)
    }
}

fn path_in(dir: &Path) -> PathBuf {
    dir.join(NAME)
}

/// Write the address, readable by its owner alone.
pub fn write(dir: &Path, address: &Address) -> Result<()> {
    fs::create_dir_all(dir).with_context(|| format!("cannot make {}", dir.display()))?;
    let path = path_in(dir);
    let temp = path.with_extension("json.tmp");

    let mut handle = open_private(&temp)?;
    handle.write_all(serde_json::to_string(address)?.as_bytes())?;
    handle.sync_all()?;
    drop(handle);

    fs::rename(&temp, &path).with_context(|| format!("cannot put {} in place", path.display()))
}

#[cfg(unix)]
fn open_private(path: &Path) -> Result<fs::File> {
    use std::os::unix::fs::OpenOptionsExt;

    // A stale temporary file keeps the mode it was made with.
    let _ = fs::remove_file(path);
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .with_context(|| format!("cannot write {}", path.display()))
}

#[cfg(not(unix))]
fn open_private(path: &Path) -> Result<fs::File> {
    fs::File::create(path).with_context(|| format!("cannot write {}", path.display()))
}

/// The address written last. Nothing says its server still runs: a crash
/// leaves the file behind, so a caller asks the server before it trusts it.
pub fn read(dir: &Path) -> Option<Address> {
    let text = fs::read_to_string(path_in(dir)).ok()?;

    serde_json::from_str(&text).ok()
}

/// Remove the address, when it is still the one this process wrote. A
/// second server that started since owns the file now.
pub fn remove(dir: &Path, mine: &Address) {
    if read(dir).as_ref() == Some(mine) {
        let _ = fs::remove_file(path_in(dir));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn address(port: u16) -> Address {
        Address {
            pid: 7,
            port,
            token: "abc".to_owned(),
        }
    }

    #[test]
    fn an_address_is_read_back() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), &address(4100)).unwrap();

        assert_eq!(read(dir.path()), Some(address(4100)));
    }

    #[cfg(unix)]
    #[test]
    fn only_its_owner_reads_the_token() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), &address(4100)).unwrap();
        let mode = fs::metadata(path_in(dir.path()))
            .unwrap()
            .permissions()
            .mode();

        assert_eq!(mode & 0o777, 0o600);
    }

    #[test]
    fn a_server_removes_its_own_address_and_no_other() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), &address(4100)).unwrap();
        write(dir.path(), &address(4200)).unwrap();

        remove(dir.path(), &address(4100));
        assert_eq!(
            read(dir.path()),
            Some(address(4200)),
            "a later server owns it"
        );

        remove(dir.path(), &address(4200));
        assert_eq!(read(dir.path()), None);
    }

    #[test]
    fn a_missing_or_broken_file_is_no_address() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(read(dir.path()), None);

        fs::write(path_in(dir.path()), "{ not json").unwrap();
        assert_eq!(read(dir.path()), None);
    }
}
