//! The command line.

use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "qreview", version, about, long_about = None)]
pub struct Cli {
    /// The port to listen on. 0 asks the system for a free one.
    #[arg(long, default_value_t = 0)]
    pub port: u16,
}
