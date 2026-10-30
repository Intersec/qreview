//! One line at the bottom of the terminal that says how the review stands.
//!
//! The messages of the server scroll away, and a reader who comes back to
//! the terminal wants the state, not the history. The line is redrawn in
//! place under the messages, on a terminal only: in a file or a pipe, the
//! messages are all there is.

use std::io::{IsTerminal, Write};

/// Clear the line the cursor is on.
const CLEAR: &str = "\r\x1b[2K";

/// How much of a subject the line shows.
const SHOWN: usize = 40;

/// The state that the line shows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Snapshot {
    /// None before the first page.
    pub page: Option<bool>,
    pub agent: Agent,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Agent {
    /// No agent came yet.
    Absent,
    Connected,
    Left,
    /// The subject of the change.
    Fixing(String),
    FixingSeries,
    /// The subject of the change that holds the thread.
    Answering(String),
}

/// The line for a state, with colors when `color` is true.
pub fn line(state: &Snapshot, color: bool) -> String {
    let page = match state.page {
        None => (Grey, "page not open yet".to_owned()),
        Some(true) => (Green, "page open".to_owned()),
        Some(false) => (Red, "page closed".to_owned()),
    };
    let agent = match &state.agent {
        Agent::Absent => (Grey, "no agent".to_owned()),
        Agent::Connected => (Green, "agent connected".to_owned()),
        Agent::Left => (
            Orange,
            "agent left: ask it to run `qreview wait`".to_owned(),
        ),
        Agent::Fixing(subject) => (Blue, format!("agent fixing “{}”", clip(subject))),
        Agent::FixingSeries => (Blue, "agent fixing the series".to_owned()),
        Agent::Answering(subject) => (Blue, format!("agent answering on “{}”", clip(subject))),
    };

    [page, agent]
        .iter()
        .map(|(dot, text)| format!("{} {text}", dot.paint(color)))
        .collect::<Vec<_>>()
        .join("   ")
}

#[derive(Clone, Copy)]
enum Dot {
    Grey,
    Green,
    Red,
    Orange,
    Blue,
}
use Dot::*;

impl Dot {
    fn paint(self, color: bool) -> String {
        let (code, mark) = match self {
            Grey => ("90", "○"),
            Green => ("32", "●"),
            Red => ("31", "●"),
            Orange => ("33", "●"),
            Blue => ("34", "●"),
        };
        if color {
            format!("\x1b[{code}m{mark}\x1b[0m")
        } else {
            mark.to_owned()
        }
    }
}

fn clip(text: &str) -> String {
    if text.chars().count() <= SHOWN {
        return text.to_owned();
    }
    let cut: String = text.chars().take(SHOWN - 1).collect();
    format!("{cut}…")
}

/// The terminal the server writes to.
pub struct Terminal {
    /// A status line is drawn. False in a file, a pipe, or under `--trace`,
    /// whose lines would break into it.
    live: bool,
    color: bool,
    drawn: Option<String>,
}

impl Terminal {
    pub fn new() -> Self {
        Self {
            live: std::io::stdout().is_terminal() && !crate::trace::on(),
            color: std::env::var_os("NO_COLOR").is_none_or(|v| v.is_empty()),
            drawn: None,
        }
    }

    /// Print a message above the status line.
    pub fn say(&mut self, message: &str) {
        let mut out = std::io::stdout().lock();
        if self.live && self.drawn.is_some() {
            let _ = write!(out, "{CLEAR}");
        }
        let _ = writeln!(out, "{message}");
        if let Some(drawn) = self.drawn.as_ref().filter(|_| self.live) {
            let _ = write!(out, "{drawn}");
        }
        let _ = out.flush();
    }

    /// Draw the status line again. Every time, and not only on a change:
    /// a line that another writer broke comes back whole.
    pub fn draw(&mut self, state: &Snapshot) {
        if !self.live {
            return;
        }
        let text = line(state, self.color);
        let mut out = std::io::stdout().lock();
        let _ = write!(out, "{CLEAR}{text}");
        let _ = out.flush();
        self.drawn = Some(text);
    }
}

impl Default for Terminal {
    fn default() -> Self {
        Self::new()
    }
}

/// Clear the status line, so the shell prompt starts on a clean line.
pub fn clear() {
    let mut out = std::io::stdout();
    if out.is_terminal() && !crate::trace::on() {
        let _ = write!(out, "{CLEAR}");
        let _ = out.flush();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_review_waits_for_its_page_and_its_agent() {
        let state = Snapshot {
            page: None,
            agent: Agent::Absent,
        };

        assert_eq!(line(&state, false), "○ page not open yet   ○ no agent");
    }

    #[test]
    fn the_work_of_the_agent_is_named_by_its_change() {
        let state = Snapshot {
            page: Some(true),
            agent: Agent::Fixing("net: retry the read".to_owned()),
        };

        assert_eq!(
            line(&state, false),
            "● page open   ● agent fixing “net: retry the read”"
        );
    }

    #[test]
    fn a_fix_of_the_series_names_no_change() {
        let state = Snapshot {
            page: Some(true),
            agent: Agent::FixingSeries,
        };

        assert_eq!(
            line(&state, false),
            "● page open   ● agent fixing the series"
        );
    }

    #[test]
    fn a_long_subject_is_cut_to_fit() {
        let state = Snapshot {
            page: Some(false),
            agent: Agent::Answering("x".repeat(60)),
        };

        let text = line(&state, false);
        assert!(
            text.contains(&format!("“{}…”", "x".repeat(SHOWN - 1))),
            "{text}"
        );
    }

    #[test]
    fn a_dot_is_colored_unless_told_not_to() {
        let state = Snapshot {
            page: Some(false),
            agent: Agent::Left,
        };

        assert!(line(&state, true).starts_with("\x1b[31m●\x1b[0m page closed"));
        assert!(!line(&state, false).contains('\x1b'));
    }
}
