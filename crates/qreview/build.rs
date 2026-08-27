//! What the binary says about itself, beyond the version Cargo knows.
//!
//! A reader who reports a problem gives a version. Between two releases that
//! version names a hundred commits, so the commit is baked in here, once, at
//! build time. A build from a tarball has no git and says nothing, which is
//! why every path below falls back to an empty string rather than failing.

use std::path::PathBuf;
use std::process::Command;

fn main() {
    // A packager who builds outside a checkout can say it instead of git.
    println!("cargo::rerun-if-env-changed=QREVIEW_GIT_SHA");

    let sha = match std::env::var("QREVIEW_GIT_SHA") {
        Ok(given) => given,
        Err(_) => head(),
    };
    let version = std::env::var("CARGO_PKG_VERSION").unwrap_or_default();
    let long = match sha.is_empty() {
        true => version,
        false => format!("{version} ({sha})"),
    };

    println!("cargo::rustc-env=QREVIEW_GIT_SHA={sha}");
    println!("cargo::rustc-env=QREVIEW_VERSION_LONG={long}");
}

/// The commit being built, short. Empty when git cannot say.
fn head() -> String {
    let Some(sha) = git(&["rev-parse", "--short=9", "HEAD"]) else {
        return String::new();
    };
    watch_the_head();

    sha
}

/// Build again when HEAD moves.
///
/// Cargo reruns this script only when a file it was told about changes.
/// `HEAD` covers a checkout, and the file the branch points at covers a
/// commit on it.
fn watch_the_head() {
    let Some(dir) = git(&["rev-parse", "--absolute-git-dir"]) else {
        return;
    };
    let dir = PathBuf::from(dir);
    println!("cargo::rerun-if-changed={}", dir.join("HEAD").display());

    if let Some(reference) = git(&["symbolic-ref", "--quiet", "HEAD"]) {
        let file = dir.join(&reference);
        // A packed ref has no file. Then HEAD alone is the trigger.
        if file.exists() {
            println!("cargo::rerun-if-changed={}", file.display());
        }
    }
}

/// Run git and take its answer, or nothing at all.
fn git(args: &[&str]) -> Option<String> {
    let out = Command::new("git").args(args).output().ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8(out.stdout).ok()?.trim().to_owned();

    (!text.is_empty()).then_some(text)
}
