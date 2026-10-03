//! The writes to the store, numbered, for whoever waits on them.
//!
//! Two parties write now: the reader in the browser and an agent in a
//! terminal. The server is the only writer of the store, so it learns of
//! every write when it makes it, and each party learns of the other's here.

use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::Duration;

use serde::Serialize;
use tokio::sync::Notify;

use crate::store::model::Author;

/// How many events are kept. A party that falls further behind reads
/// everything again.
const KEPT: usize = 512;

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Kind {
    Comment,
    Reply,
    Edited,
    Done,
    Deleted,
    Refresh,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Event {
    pub seq: u64,
    pub kind: Kind,
    pub author: Author,
    /// The change. Empty on a refresh, which is about the whole series.
    pub key: String,
    pub id: Option<String>,
}

/// What one wait answers.
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Batch {
    pub events: Vec<Event>,
    /// The number to ask after next time.
    pub next: u64,
    /// The events asked for are gone, or were never made by this server.
    /// The party reads everything again.
    pub reset: bool,
}

#[derive(Default)]
pub struct Events {
    log: Mutex<Log>,
    notify: Notify,
    /// The server stops. A page always holds a wait open, and a stop must
    /// not sit behind it.
    closed: std::sync::atomic::AtomicBool,
}

#[derive(Default)]
struct Log {
    /// The number of the last event, 0 before the first.
    last: u64,
    kept: VecDeque<Event>,
}

impl Events {
    pub fn push(&self, kind: Kind, author: Author, key: &str, id: Option<&str>) {
        {
            let mut log = self.log.lock().expect("the event log is poisoned");
            log.last += 1;
            let event = Event {
                seq: log.last,
                kind,
                author,
                key: key.to_owned(),
                id: id.map(str::to_owned),
            };
            log.kept.push_back(event);
            if log.kept.len() > KEPT {
                log.kept.pop_front();
            }
        }
        self.notify.notify_waiters();
    }

    /// The events after `after`, as soon as there is one, or none once
    /// `wait` has passed. With no `after`, the number to start from, at once.
    /// Answer every wait now, and every later one at once.
    pub fn close(&self) {
        self.closed.store(true, std::sync::atomic::Ordering::SeqCst);
        self.notify.notify_waiters();
    }

    pub async fn after(&self, after: Option<u64>, wait: Duration) -> Batch {
        let Some(after) = after else {
            return self.since(u64::MAX);
        };

        let notified = self.notify.notified();
        tokio::pin!(notified);
        // Registered before the log is read, so a push between the read and
        // the wait still wakes this one.
        notified.as_mut().enable();

        let batch = self.since(after);
        let closed = self.closed.load(std::sync::atomic::Ordering::SeqCst);
        if !batch.events.is_empty() || batch.reset || closed {
            return batch;
        }
        let _ = tokio::time::timeout(wait, notified).await;

        self.since(after)
    }

    fn since(&self, after: u64) -> Batch {
        let log = self.log.lock().expect("the event log is poisoned");
        let next = log.last;

        if after == u64::MAX {
            return Batch {
                events: Vec::new(),
                next,
                reset: false,
            };
        }
        let first = log.kept.front().map_or(log.last + 1, |e| e.seq);
        // Ahead of this server: it restarted. Behind what is kept: the
        // events fell out of memory.
        if after > log.last || after + 1 < first {
            return Batch {
                events: Vec::new(),
                next,
                reset: true,
            };
        }
        Batch {
            events: log.kept.iter().filter(|e| e.seq > after).cloned().collect(),
            next,
            reset: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;

    const SHORT: Duration = Duration::from_millis(20);

    #[tokio::test]
    async fn no_number_answers_at_once_with_the_one_to_start_from() {
        let events = Events::default();
        events.push(Kind::Comment, Author::Reader, "I1", Some("c-1"));

        let batch = events.after(None, Duration::from_secs(30)).await;

        assert_eq!(batch.events, []);
        assert_eq!(batch.next, 1);
        assert!(!batch.reset);
    }

    #[tokio::test]
    async fn the_events_after_a_number_come_back_at_once() {
        let events = Events::default();
        events.push(Kind::Comment, Author::Reader, "I1", Some("c-1"));
        events.push(Kind::Reply, Author::Agent, "I1", Some("c-2"));

        let batch = events.after(Some(1), SHORT).await;

        assert_eq!(batch.events.len(), 1);
        assert_eq!(batch.events[0].kind, Kind::Reply);
        assert_eq!(batch.events[0].author, Author::Agent);
        assert_eq!(batch.next, 2);
    }

    #[tokio::test]
    async fn a_wait_ends_empty_when_nothing_comes() {
        let events = Events::default();

        let batch = events.after(Some(0), SHORT).await;

        assert_eq!(batch.events, []);
        assert_eq!(batch.next, 0);
        assert!(!batch.reset);
    }

    #[tokio::test]
    async fn a_push_wakes_a_wait() {
        let events = Arc::new(Events::default());
        let waiting = {
            let events = events.clone();
            tokio::spawn(async move { events.after(Some(0), Duration::from_secs(30)).await })
        };
        tokio::time::sleep(SHORT).await;

        events.push(Kind::Done, Author::Agent, "I1", Some("c-1"));
        let batch = tokio::time::timeout(Duration::from_secs(5), waiting)
            .await
            .expect("the push must end the wait")
            .unwrap();

        assert_eq!(batch.events.len(), 1);
    }

    #[tokio::test]
    async fn a_closed_log_answers_every_wait_at_once() {
        let events = Arc::new(Events::default());
        let waiting = {
            let events = events.clone();
            tokio::spawn(async move { events.after(Some(0), Duration::from_secs(30)).await })
        };
        tokio::time::sleep(SHORT).await;

        events.close();

        tokio::time::timeout(Duration::from_secs(5), waiting)
            .await
            .expect("a stop must not wait for a poll")
            .unwrap();
        let later = tokio::time::timeout(
            Duration::from_secs(5),
            events.after(Some(0), Duration::from_secs(30)),
        )
        .await;
        assert!(
            later.is_ok(),
            "a poll that comes after the stop answers too"
        );
    }

    #[tokio::test]
    async fn a_number_this_server_never_gave_reads_everything_again() {
        let events = Events::default();

        assert!(events.after(Some(7), SHORT).await.reset);
    }

    #[tokio::test]
    async fn a_number_older_than_what_is_kept_reads_everything_again() {
        let events = Events::default();
        for _ in 0..KEPT + 2 {
            events.push(Kind::Comment, Author::Reader, "I1", None);
        }

        assert!(events.after(Some(1), SHORT).await.reset);
        assert!(!events.after(Some(2), SHORT).await.reset);
    }
}
