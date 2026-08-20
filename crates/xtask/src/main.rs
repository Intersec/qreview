//! Release and changelog chores.
//!
//! `cargo xtask release` collects the changelog fragments, writes
//! CHANGELOG.md, bumps the version, and tags. It arrives in M7. See
//! `roadmap/plan.md`.

use anyhow::{Result, bail};

fn main() -> Result<()> {
    match std::env::args().nth(1).as_deref() {
        Some("release") => bail!("release is not implemented yet, see roadmap/plan.md M7"),
        Some(task) => bail!("unknown task {task:?}, expected: release"),
        None => {
            println!("usage: cargo xtask <task>");
            println!();
            println!("tasks:");
            println!("  release   cut a version from the changelog fragments");
            Ok(())
        }
    }
}
