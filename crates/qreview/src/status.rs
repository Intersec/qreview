//! One line at the bottom of the terminal that says how the review stands.
//!
//! The messages of the server scroll away, and a reader who comes back to
//! the terminal wants the state, not the history. The line is redrawn in
//! place under the messages, on a terminal only: in a file or a pipe, the
//! messages are all there is.

use std::io::{IsTerminal, Write};
use std::sync::atomic::{AtomicUsize, Ordering};

/// Clear the line the cursor is on.
const CLEAR: &str = "\r\x1b[2K";

/// How much of a subject the line shows.
const SHOWN: usize = 40;

/// Programs that ask the user in the terminal now. The line stays away
/// meanwhile: its redraw would erase their question.
static LENT: AtomicUsize = AtomicUsize::new(0);

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
    /// The messages said while the terminal was lent, which would have
    /// landed in the middle of a question.
    held: Vec<Held>,
}

/// A message said while the terminal was lent, and its stream.
#[derive(Debug, PartialEq, Eq)]
enum Held {
    Say(String),
    Warn(String),
}

impl Terminal {
    pub fn new() -> Self {
        Self {
            live: std::io::stdout().is_terminal() && !crate::trace::on(),
            color: std::env::var_os("NO_COLOR").is_none_or(|v| v.is_empty()),
            drawn: None,
            held: Vec::new(),
        }
    }

    /// Print a message above the status line.
    pub fn say(&mut self, message: &str) {
        self.out(Held::Say(message.to_owned()));
    }

    /// Print a warning above the status line, on the error stream.
    pub fn warn(&mut self, message: &str) {
        self.out(Held::Warn(message.to_owned()));
    }

    fn out(&mut self, message: Held) {
        if lent() {
            self.held.push(message);
            return;
        }
        self.say_held();
        self.print(&message);
    }

    /// The messages held while the terminal was lent, in their order.
    fn say_held(&mut self) {
        for message in std::mem::take(&mut self.held) {
            self.print(&message);
        }
    }

    fn print(&self, message: &Held) {
        match message {
            Held::Say(text) => self.above(|| println!("{text}")),
            Held::Warn(text) => self.above(|| eprintln!("qreview: {text}")),
        }
    }

    /// Clear the status line, print, and draw it again under the print.
    fn above(&self, print: impl FnOnce()) {
        let live = self.live && self.drawn.is_some();
        if live {
            let mut out = std::io::stdout().lock();
            let _ = write!(out, "{CLEAR}");
            let _ = out.flush();
        }
        print();
        if let Some(drawn) = self.drawn.as_ref().filter(|_| live) {
            let mut out = std::io::stdout().lock();
            let _ = write!(out, "{drawn}");
            let _ = out.flush();
        }
    }

    /// Draw the status line again. Every time, and not only on a change:
    /// a line that another writer broke comes back whole.
    pub fn draw(&mut self, state: &Snapshot) {
        if lent() {
            return;
        }
        self.say_held();
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

/// What the server has to say about the configuration while it runs.
///
/// The themes are read again at each page load. A file that stays broken
/// must not be said at each reload, so only a note that was not said the
/// last time is queued.
#[derive(Debug, Default)]
pub struct Notes(std::sync::Mutex<NoteState>);

#[derive(Debug, Default)]
struct NoteState {
    said: Vec<String>,
    pending: Vec<String>,
}

impl Notes {
    /// The notes that the terminal already shows, at startup.
    pub fn said(notes: &[String]) -> Self {
        Self(std::sync::Mutex::new(NoteState {
            said: notes.to_vec(),
            pending: Vec::new(),
        }))
    }

    /// The notes of the configuration now. The new ones are queued.
    pub fn offer(&self, notes: &[String]) {
        let mut state = self.0.lock().expect("the notes lock is poisoned");
        let NoteState { said, pending } = &mut *state;
        pending.extend(notes.iter().filter(|n| !said.contains(n)).cloned());
        *said = notes.to_vec();
    }

    /// The notes to print, once each.
    pub fn take(&self) -> Vec<String> {
        let mut state = self.0.lock().expect("the notes lock is poisoned");
        std::mem::take(&mut state.pending)
    }
}

impl Default for Terminal {
    fn default() -> Self {
        Self::new()
    }
}

/// Lend the terminal to a program that may ask the user: the status line
/// clears now, and comes back when the guard drops.
pub fn lend() -> Lent {
    LENT.fetch_add(1, Ordering::SeqCst);
    clear();
    Lent
}

fn lent() -> bool {
    LENT.load(Ordering::SeqCst) > 0
}

/// The terminal is lent while it lives.
pub struct Lent;

impl Drop for Lent {
    fn drop(&mut self) {
        LENT.fetch_sub(1, Ordering::SeqCst);
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
    fn a_note_is_said_once_while_it_holds() {
        let notes = Notes::said(&["broken.css:3".to_owned()]);

        notes.offer(&["broken.css:3".to_owned()]);
        assert!(notes.take().is_empty());

        notes.offer(&["broken.css:3".to_owned(), "gone.css:1".to_owned()]);
        assert_eq!(notes.take(), ["gone.css:1"]);
        assert!(notes.take().is_empty());

        // Fixed, then broken again: said again.
        notes.offer(&[]);
        notes.offer(&["gone.css:1".to_owned()]);
        assert_eq!(notes.take(), ["gone.css:1"]);
    }

    #[test]
    fn a_message_waits_while_a_program_asks_the_user() {
        let mut terminal = Terminal {
            live: false,
            color: false,
            drawn: None,
            held: Vec::new(),
        };

        {
            let _lent = lend();
            terminal.say("The review page is open.");
            terminal.warn("A theme file is broken.");
            assert_eq!(
                terminal.held,
                [
                    Held::Say("The review page is open.".to_owned()),
                    Held::Warn("A theme file is broken.".to_owned()),
                ]
            );
        }
        terminal.say_held();

        assert!(terminal.held.is_empty());
    }

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
