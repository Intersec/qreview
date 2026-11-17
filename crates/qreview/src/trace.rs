//! Timings, and what the server waits on now.
//!
//! A review of a large repository can feel slow, and the cost is never where
//! a reader guesses. Each piece of work makes one line. `--trace` prints the
//! lines to standard error, as they come. Standard output belongs to
//! `export`.
//!
//! The server keeps the last lines in memory in any case, so the page shows
//! them in its Trace window without a restart. A line costs a string, which
//! is nothing beside the child process it describes.
//!
//! The slow calls, those that cross the network, also say when they start:
//! the page shows what the server waits on while it waits.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

use serde::Serialize;

static ON: AtomicBool = AtomicBool::new(false);
static ORIGIN: OnceLock<Instant> = OnceLock::new();

/// How many lines the server keeps for the page.
const KEPT: usize = 2000;

/// The name of the variable that switches the timings on.
pub const ENV: &str = "QREVIEW_TRACE";

/// One line of the trace.
#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct Line {
    /// Its number, from 1. The page asks for the lines after the last one
    /// it has.
    pub n: u64,
    /// Seconds since the start of the run.
    pub at: f64,
    /// What the work cost, in milliseconds. None for an event.
    pub ms: Option<f64>,
    pub what: String,
}

struct Kept {
    lines: VecDeque<Line>,
    next: u64,
}

fn kept() -> &'static Mutex<Kept> {
    static KEPT_LINES: OnceLock<Mutex<Kept>> = OnceLock::new();
    KEPT_LINES.get_or_init(|| {
        Mutex::new(Kept {
            lines: VecDeque::new(),
            next: 1,
        })
    })
}

fn origin() -> Instant {
    *ORIGIN.get_or_init(Instant::now)
}

/// Print the lines on standard error too, and fix the origin of the clock.
pub fn enable() {
    origin();
    ON.store(true, Ordering::Relaxed);
}

/// Switch them on when the environment asks for it.
///
/// Any value but the empty string and `0` counts as yes.
pub fn enable_from_env() {
    match std::env::var(ENV) {
        Ok(value) if !value.is_empty() && value != "0" => enable(),
        _ => {}
    }
}

/// Whether this run prints timings.
pub fn on() -> bool {
    ON.load(Ordering::Relaxed)
}

/// The clock a block of work starts with.
pub fn start() -> Option<Instant> {
    Some(Instant::now())
}

/// Write down what a block of work cost.
pub fn since(started: Option<Instant>, what: impl FnOnce() -> String) {
    let Some(started) = started else {
        return;
    };
    let ms = Instant::now().duration_since(started).as_secs_f64() * 1000.0;
    line(Some(ms), what());
}

/// Write down an event that has no duration.
pub fn note(what: impl FnOnce() -> String) {
    line(None, what());
}

fn line(ms: Option<f64>, what: String) {
    let at = origin().elapsed().as_secs_f64();
    if on() {
        match ms {
            Some(ms) => eprintln!("trace {at:8.3}s {ms:8.1}ms  {what}"),
            None => eprintln!("trace {at:8.3}s {:10}  {what}", ""),
        }
    }

    let mut kept = kept()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let n = kept.next;
    kept.next += 1;
    kept.lines.push_back(Line { n, at, ms, what });
    while kept.lines.len() > KEPT {
        kept.lines.pop_front();
    }
}

/// The lines after the number `after`, oldest first, and the number of the
/// next line to come.
pub fn lines_after(after: u64) -> (Vec<Line>, u64) {
    let kept = kept()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let lines = kept
        .lines
        .iter()
        .filter(|line| line.n > after)
        .cloned()
        .collect();
    (lines, kept.next)
}

/// A slow call in flight, such as a query to Gerrit.
#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct Waiting {
    pub what: String,
    /// How long it has run, in milliseconds.
    pub ms: f64,
}

fn waiting() -> &'static Mutex<Vec<(u64, String, Instant)>> {
    static WAITING: OnceLock<Mutex<Vec<(u64, String, Instant)>>> = OnceLock::new();
    WAITING.get_or_init(|| Mutex::new(Vec::new()))
}

static NEXT_WAIT: AtomicU64 = AtomicU64::new(1);

/// Say that a slow call starts. It ends when the guard is dropped.
pub fn wait(what: impl Into<String>) -> Busy {
    let id = NEXT_WAIT.fetch_add(1, Ordering::Relaxed);
    waiting()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .push((id, what.into(), Instant::now()));
    Busy { id }
}

/// A slow call in flight. Dropped, it is over.
pub struct Busy {
    id: u64,
}

impl Drop for Busy {
    fn drop(&mut self) {
        waiting()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .retain(|(id, _, _)| *id != self.id);
    }
}

/// The slow calls in flight, oldest first.
pub fn waiting_now() -> Vec<Waiting> {
    let now = Instant::now();
    waiting()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .iter()
        .map(|(_, what, since)| Waiting {
            what: what.clone(),
            ms: now.duration_since(*since).as_secs_f64() * 1000.0,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_lines_are_kept_for_the_page_and_read_after_a_number() {
        note(|| "first of this test".to_owned());
        let (lines, next) = lines_after(0);
        let first = lines
            .iter()
            .find(|line| line.what == "first of this test")
            .unwrap()
            .n;

        note(|| "second of this test".to_owned());
        let (after, later) = lines_after(first);
        assert!(after.iter().any(|line| line.what == "second of this test"));
        assert!(after.iter().all(|line| line.n > first));
        assert!(later > next);
    }

    #[test]
    fn a_slow_call_is_waited_on_until_it_ends() {
        let busy = wait("ssh review.example.com gerrit query in this test");
        assert!(
            waiting_now()
                .iter()
                .any(|w| w.what == "ssh review.example.com gerrit query in this test")
        );

        drop(busy);
        assert!(
            !waiting_now()
                .iter()
                .any(|w| w.what == "ssh review.example.com gerrit query in this test")
        );
    }
}
