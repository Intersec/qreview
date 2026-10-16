//! The command line.

use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(name = "qreview", version = qreview::version::LONG, about, long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,

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

    /// Do not show the tracked changes that are not committed.
    #[arg(long)]
    pub no_worktree: bool,

    /// Print the URL, open no browser.
    #[arg(long)]
    pub no_open: bool,

    /// Print the series and the files of every change before the URL.
    #[arg(short, long)]
    pub verbose: bool,

    /// Print, on standard error, what every piece of work costs.
    #[arg(long)]
    pub trace: bool,

    /// The port to listen on. 0 asks the system for a free one.
    #[arg(long, default_value_t = 0)]
    pub port: u16,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Print the review as Markdown, to paste into a session.
    Export {
        /// One change. The whole series when it is missing.
        #[arg(long, value_name = "CHANGE-ID")]
        key: Option<String>,
        /// The open threads as JSON, each comment with its id.
        #[arg(long)]
        json: bool,
    },

    /// List the changes this repository has a review for.
    List,

    /// Write a remark as the agent, on the server of this repository.
    Comment {
        /// `<file>`, `<file>:<side>:<line>` or `<file>:<side>:<from>-<to>`.
        /// The side is `old` or `new`.
        place: String,
        /// The text. `-` reads it from standard input.
        #[arg(long)]
        body: String,
        /// The change. The newest change that touches the file by default.
        #[arg(long, value_name = "CHANGE-ID")]
        key: Option<String>,
    },

    /// Reply to a thread as the agent.
    Reply {
        /// The id of a comment of the thread.
        id: String,
        /// The text. `-` reads it from standard input.
        #[arg(long)]
        body: String,
        /// Check the Done box of the thread.
        #[arg(long)]
        done: bool,
        /// Wait for the reader: a question, or a problem to solve with them.
        #[arg(long)]
        blocked: bool,
    },

    /// Wait for the next remarks and replies of the reader, and print them
    /// as JSON. Exits 1 when the time runs out.
    Wait {
        /// The `next` number of the last wait. Now by default.
        #[arg(long)]
        after: Option<u64>,
        /// Seconds to wait.
        #[arg(long, default_value_t = 600)]
        timeout: u64,
    },

    /// Read the repository again, as the refresh button does.
    Refresh,

    /// Declare a commit the new version of another one, after an amend, and
    /// move the threads of the old version onto it.
    Migrate {
        /// The commit before the amend.
        #[arg(long, value_name = "SHA")]
        from: String,
        /// The commit after the amend. HEAD by default.
        #[arg(long, value_name = "SHA")]
        to: Option<String>,
        /// Threads to close with a Done reply, as ids separated by commas.
        #[arg(long, value_name = "IDS")]
        done: Vec<String>,
        /// A thread to ask the reader about, as `<id>=<text>`. Repeatable.
        #[arg(long, value_name = "ID=TEXT")]
        blocked: Vec<String>,
    },

    /// Print the procedure that an agent follows to review with this
    /// release.
    Skill,
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    /// The subcommands that the procedure names in its code: inline code
    /// that opens with `qreview `, and the lines of a code block.
    fn named_commands(text: &str) -> Vec<String> {
        let mut found = Vec::new();
        let mut in_block = false;

        for line in text.lines() {
            if line.trim_start().starts_with("```") {
                in_block = !in_block;
                continue;
            }
            let calls: Vec<&str> = match in_block {
                true => line.strip_prefix("qreview ").into_iter().collect(),
                false => line.split("`qreview ").skip(1).collect(),
            };
            for call in calls {
                let word: String = call
                    .chars()
                    .take_while(|c| c.is_ascii_lowercase() || *c == '-')
                    .collect();
                // An option or a placeholder such as `<rev>` is no command.
                if !word.is_empty() && !word.starts_with('-') {
                    found.push(word);
                }
            }
        }
        found
    }

    #[test]
    fn the_procedure_names_only_commands_of_this_release() {
        let cli = Cli::command();
        let named = named_commands(qreview::agent::SKILL);

        assert!(named.contains(&"wait".to_owned()), "{named:?}");
        for word in named {
            assert!(
                cli.find_subcommand(&word).is_some(),
                "the procedure names `qreview {word}`, which this release does not have"
            );
        }
    }

    #[test]
    fn a_placeholder_or_an_option_is_no_command() {
        let text = "Run `qreview <rev>` or `qreview --base x`.\n```sh\nqreview wait\n```\n";

        assert_eq!(named_commands(text), ["wait"]);
    }
}
