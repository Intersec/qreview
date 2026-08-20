//! Cutting a version.

use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result, bail};

use crate::changelog;

pub fn run(root: &Path, version: &str) -> Result<()> {
    check_version(version)?;
    check_clean(root)?;

    let collected = changelog::collect(root)?;
    if collected.files.is_empty() {
        bail!("nothing is waiting for a release. See changelog/README.md");
    }

    bump(root, version)?;
    write_changelog(root, version, &collected.text)?;

    for file in &collected.files {
        std::fs::remove_file(file).with_context(|| format!("cannot remove {}", file.display()))?;
    }

    git(root, &["add", "-A"])?;
    git(
        root,
        &["commit", "-m", &format!("chore(release): v{version}")],
    )?;
    git(root, &["tag", &format!("v{version}")])?;

    println!("v{version} is cut and tagged. Nothing is pushed: that is yours to do.");
    Ok(())
}

fn check_version(version: &str) -> Result<()> {
    let parts: Vec<_> = version.split('.').collect();
    let ok = parts.len() == 3
        && parts
            .iter()
            .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()));

    if !ok {
        bail!("{version} is not a version like 0.1.0");
    }
    Ok(())
}

fn check_clean(root: &Path) -> Result<()> {
    let out = Command::new("git")
        .current_dir(root)
        .args(["status", "--porcelain"])
        .output()
        .context("cannot run git")?;

    if !out.stdout.is_empty() {
        bail!("the working tree is not clean. Commit or stash first");
    }
    Ok(())
}

/// The version lives in the workspace manifest, alone.
fn bump(root: &Path, version: &str) -> Result<()> {
    let path = root.join("Cargo.toml");
    let text = std::fs::read_to_string(&path)?;
    let mut out = String::with_capacity(text.len());
    let mut done = false;

    for line in text.lines() {
        if !done && line.starts_with("version = ") {
            out.push_str(&format!("version = \"{version}\"\n"));
            done = true;
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }

    if !done {
        bail!("no version line in {}", path.display());
    }
    std::fs::write(&path, out)?;

    // Keep Cargo.lock in step, so the release commit holds both.
    Command::new("cargo")
        .current_dir(root)
        .args(["update", "--workspace", "--offline"])
        .status()
        .context("cannot update Cargo.lock")?;

    Ok(())
}

fn write_changelog(root: &Path, version: &str, entries: &str) -> Result<()> {
    let path = root.join("CHANGELOG.md");
    let text = std::fs::read_to_string(&path)?;
    let marker = "<!-- The release script writes new versions under this line. -->";

    let Some(at) = text.find(marker) else {
        bail!("no marker in {}. See CHANGELOG.md", path.display());
    };
    let cut = at + marker.len();
    let section = format!("\n\n## [{version}]\n\n{}", entries.trim_end());

    let mut out = String::with_capacity(text.len() + section.len());
    out.push_str(&text[..cut]);
    out.push_str(&section);
    out.push('\n');
    out.push_str(text[cut..].trim_start_matches('\n'));

    std::fs::write(&path, out)?;
    Ok(())
}

fn git(root: &Path, args: &[&str]) -> Result<()> {
    let status = Command::new("git")
        .current_dir(root)
        .args(args)
        .status()
        .with_context(|| format!("cannot run git {}", args.join(" ")))?;

    if !status.success() {
        bail!("git {} failed", args.join(" "));
    }
    Ok(())
}
