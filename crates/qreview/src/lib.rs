//! qreview — local code review for a Git series, with the Gerrit model.
//!
//! The binary is a thin shell over this library: it parses the arguments,
//! starts the server, and opens the browser. Everything the review needs
//! lives here, so it can be tested without a process.

pub mod api;
pub mod assets;
pub mod diff;
pub mod git;
pub mod lang;
pub mod model;
pub mod repo;
pub mod report;
pub mod series;
pub mod session;

#[cfg(test)]
mod testutil;
