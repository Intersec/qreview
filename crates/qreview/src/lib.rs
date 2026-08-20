//! qreview — local code review for a Git series, with the Gerrit model.
//!
//! The binary is a thin shell over this library: it parses the arguments,
//! starts the server, and opens the browser. Everything the review needs
//! lives here, so it can be tested without a process.

pub mod assets;
pub mod git;
pub mod model;
pub mod series;

#[cfg(test)]
mod testutil;
