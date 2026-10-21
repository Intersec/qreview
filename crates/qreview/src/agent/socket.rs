//! The Unix socket of a server, for the commands of an agent.
//!
//! An agent in a sandbox runs each command in a network of its own, where
//! `127.0.0.1` is not the machine of the user, so the loopback port is out
//! of its reach. A socket file under `/tmp` is not: a sandbox shares `/tmp`
//! with the machine. The directory belongs to the user and nobody else can
//! enter it, so the socket opens to the user alone. The token is still
//! asked for, as on the port.

use std::fs;
use std::os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use super::Home;

/// `/tmp` itself, and not `$TMPDIR`: a sandbox points `$TMPDIR` somewhere
/// of its own, and the server and the agent must name the same place.
const ROOT: &str = "/tmp";

/// The socket of the server of a worktree, its directory made safe first.
pub fn path_for(home: &Home) -> Result<PathBuf> {
    path_in(Path::new(ROOT), home)
}

fn path_in(root: &Path, home: &Home) -> Result<PathBuf> {
    let uid = fs::metadata("/proc/self")
        .context("cannot read who runs this process")?
        .uid();
    let dir = root.join(format!("qreview-{uid}"));

    private_dir(&dir, uid)?;
    Ok(dir.join(format!("{}-{}.sock", home.repo, home.worktree)))
}

/// Make the directory, or check the one that is there.
///
/// Anybody can create a name under `/tmp`. A directory that another user
/// made, or that others can enter, could hand the token to them, so it is
/// refused rather than used.
fn private_dir(dir: &Path, uid: u32) -> Result<()> {
    match fs::DirBuilder::new().mode(0o700).create(dir) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(error) => {
            return Err(error).with_context(|| format!("cannot make {}", dir.display()));
        }
    }

    let meta =
        fs::symlink_metadata(dir).with_context(|| format!("cannot read {}", dir.display()))?;
    if !meta.is_dir() {
        bail!("{} is not a directory. Remove it", dir.display());
    }
    if meta.uid() != uid {
        bail!("{} belongs to another user. Remove it", dir.display());
    }
    if meta.permissions().mode() & 0o077 != 0 {
        bail!(
            "others can enter {}. Run `chmod 700 {}`",
            dir.display(),
            dir.display()
        );
    }
    Ok(())
}

/// Remove a socket that a server left behind, so a new one can bind. Only a
/// socket: never another kind of file that happens to have the name.
pub fn clear(path: &Path) {
    use std::os::unix::fs::FileTypeExt;

    if fs::symlink_metadata(path).is_ok_and(|meta| meta.file_type().is_socket()) {
        let _ = fs::remove_file(path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn uid() -> u32 {
        fs::metadata("/proc/self").unwrap().uid()
    }

    #[test]
    fn a_new_directory_is_made_for_the_user_alone() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("qreview-x");

        private_dir(&dir, uid()).unwrap();

        let mode = fs::metadata(&dir).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o700);
    }

    #[test]
    fn a_directory_others_can_enter_is_refused() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("qreview-x");
        fs::create_dir(&dir).unwrap();
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o755)).unwrap();

        let error = private_dir(&dir, uid()).unwrap_err();

        assert!(error.to_string().contains("chmod 700"), "{error}");
    }

    #[test]
    fn a_directory_of_another_user_is_refused() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("qreview-x");
        private_dir(&dir, uid()).unwrap();

        let error = private_dir(&dir, uid() + 1).unwrap_err();

        assert!(error.to_string().contains("another user"), "{error}");
    }

    #[test]
    fn a_file_in_the_way_is_refused_and_never_removed() {
        let root = tempfile::tempdir().unwrap();
        let file = root.path().join("qreview-x");
        fs::write(&file, "mine").unwrap();

        assert!(private_dir(&file, uid()).is_err());
        clear(&file);
        assert_eq!(fs::read_to_string(&file).unwrap(), "mine", "not a socket");
    }

    #[test]
    fn the_socket_is_named_after_the_repository_and_the_worktree() {
        let root = tempfile::tempdir().unwrap();
        let home = Home {
            dir: PathBuf::from("/home/u/.local/state/qreview/repos/4f59"),
            repo: "4f59".to_owned(),
            worktree: "a1b2".to_owned(),
        };

        let path = path_in(root.path(), &home).unwrap();

        assert_eq!(path.file_name().unwrap(), "4f59-a1b2.sock");
        assert_eq!(
            path.parent().unwrap(),
            root.path().join(format!("qreview-{}", uid()))
        );
    }
}
