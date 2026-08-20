//! The command line.

use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "qreview", version, about, long_about = None)]
pub struct Cli {
    /// A revision, or a `revA..revB` range. The current series by default.
    pub rev: Option<String>,

    /// The base of the series. It wins over every other rule.
    #[arg(long, value_name = "REV")]
    pub base: Option<String>,

    /// A commit to treat as an older patch set of a change. Repeatable.
    #[arg(long = "prev", value_name = "SHA")]
    pub prev: Vec<String>,

    /// Do not ask Gerrit for the patch sets already pushed.
    #[arg(long)]
    pub no_gerrit: bool,

    /// Print the URL, open no browser.
    #[arg(long)]
    pub no_open: bool,

    /// The port to listen on. 0 asks the system for a free one.
    #[arg(long, default_value_t = 0)]
    pub port: u16,
}
