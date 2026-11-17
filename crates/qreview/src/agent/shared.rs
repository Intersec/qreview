//! `<git-dir>/qreview/`: the way in for an agent in a container.
//!
//! A devcontainer shares the project with the machine of the user, at the
//! same path, but not `/tmp` nor the store. So the server puts a second
//! socket and a copy of its address in the git directory of the worktree,
//! and the commands of the agent look there first. Only this repository is
//! exposed, and only while the server runs. Any process that reads the
//! repository can then drive this review, as the page does.

use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use super::address::{self, Address};
use super::socket;

/// The directory, in the git directory of the worktree.
pub fn dir(git_dir: &Path) -> PathBuf {
    git_dir.join("qreview")
}

fn address_path(git_dir: &Path) -> PathBuf {
    dir(git_dir).join("server.json")
}

/// The socket, its directory made private first.
pub fn socket_path(git_dir: &Path) -> Result<PathBuf> {
    let dir = dir(git_dir);
    let uid = fs::metadata("/proc/self")
        .context("cannot read who runs this process")?
        .uid();
    socket::private_dir(&dir, uid)?;
    Ok(dir.join("agent.sock"))
}

/// The longest path that a Unix socket takes, with its final zero byte.
const SOCKET_PATH_MAX: usize = 107;

/// A path to bind or connect the socket at: the path itself, or, when it is
/// too long for a socket, the same file relative to the working directory.
pub fn reachable(path: &Path) -> Result<PathBuf> {
    if path.as_os_str().len() <= SOCKET_PATH_MAX {
        return Ok(path.to_path_buf());
    }
    let cwd = std::env::current_dir().context("cannot read the working directory")?;
    let relative = relative_to(path, &cwd);
    if relative.as_os_str().len() > SOCKET_PATH_MAX {
        bail!(
            "{} is longer than a Unix socket path can be",
            path.display()
        );
    }
    Ok(relative)
}

/// `path` seen from `from`, both absolute.
fn relative_to(path: &Path, from: &Path) -> PathBuf {
    let path: Vec<_> = path.components().collect();
    let from: Vec<_> = from.components().collect();
    let common = path.iter().zip(&from).take_while(|(a, b)| a == b).count();

    let mut out = PathBuf::new();
    for _ in common..from.len() {
        out.push("..");
    }
    for part in &path[common..] {
        out.push(part);
    }
    out
}

/// Write the copy of the address, which names the socket of the git
/// directory when there is one.
pub fn write(git_dir: &Path, address: &Address) -> Result<()> {
    address::write_at(&address_path(git_dir), address)
}

/// The address that the copy holds.
pub fn read(git_dir: &Path) -> Option<Address> {
    let text = fs::read_to_string(address_path(git_dir)).ok()?;
    serde_json::from_str(&text).ok()
}

/// Remove the copy, when it is still the one this server wrote.
pub fn remove(git_dir: &Path, mine: &Address) {
    if read(git_dir).as_ref() == Some(mine) {
        let _ = fs::remove_file(address_path(git_dir));
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
            socket: Some("/repo/.git/qreview/agent.sock".to_owned()),
        }
    }

    #[test]
    fn the_copy_is_private_and_read_back() {
        use std::os::unix::fs::PermissionsExt;

        let git_dir = tempfile::tempdir().unwrap();
        socket_path(git_dir.path()).unwrap();
        write(git_dir.path(), &address(4100)).unwrap();

        assert_eq!(read(git_dir.path()), Some(address(4100)));
        let dir = fs::metadata(dir(git_dir.path())).unwrap().permissions();
        assert_eq!(dir.mode() & 0o777, 0o700);
        let file = fs::metadata(address_path(git_dir.path()))
            .unwrap()
            .permissions();
        assert_eq!(file.mode() & 0o777, 0o600);
    }

    #[test]
    fn a_server_removes_its_own_copy_and_no_other() {
        let git_dir = tempfile::tempdir().unwrap();
        socket_path(git_dir.path()).unwrap();
        write(git_dir.path(), &address(4100)).unwrap();
        write(git_dir.path(), &address(4200)).unwrap();

        remove(git_dir.path(), &address(4100));
        assert_eq!(read(git_dir.path()), Some(address(4200)));
        remove(git_dir.path(), &address(4200));
        assert_eq!(read(git_dir.path()), None);
    }

    #[test]
    fn a_long_path_is_reached_from_the_working_directory() {
        let cwd = Path::new("/home/u/project");
        let long = Path::new("/home/u/project/.git/worktrees/w/qreview/agent.sock");

        assert_eq!(
            relative_to(long, cwd),
            Path::new(".git/worktrees/w/qreview/agent.sock")
        );
        assert_eq!(
            relative_to(Path::new("/home/u/other/.git/qreview/agent.sock"), cwd),
            Path::new("../other/.git/qreview/agent.sock")
        );
        let short = Path::new("/r/.git/qreview/agent.sock");
        assert_eq!(reachable(short).unwrap(), short);
    }
}
