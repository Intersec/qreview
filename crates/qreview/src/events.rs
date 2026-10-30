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
    /// The reader asks the agent to answer one thread.
    Ask,
    /// A new version was declared and the threads moved onto it.
    Migrated,
    /// The reader asks the agent to fix the change, or the whole series
    /// when the key is empty.
    Fix,
    /// The reader deleted the done threads of the change.
    Clean,
    /// The agent waits again after it wrote: its work is over, and a page
    /// that held its reloads catches up once.
    Idle,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Event {
    pub seq: u64,
    pub kind: Kind,
    pub author: Author,
    /// The change. Empty on a refresh and on a fix of the series, which are
    /// about the whole series.
    pub key: String,
    pub id: Option<String>,
    /// On a `migrated` event: the old version, the new one, and the change
    /// the old one was kept under, so a page can move what it holds of it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from_key: Option<String>,
}

/// The versions a `migrated` event names.
pub struct Versions<'a> {
    pub from: &'a str,
    pub to: &'a str,
    pub from_key: &'a str,
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
    /// An agent listens: a `qreview wait` is open, or ended a moment ago.
    pub agent: bool,
    /// The agent works on a fix: the reader asked for one, and the agent has
    /// not waited again since.
    pub working: bool,
    /// An agent listened to this server, and listens no more. The page asks
    /// the reader to start it again.
    pub left: bool,
    /// What the agent does now, so the page can name it.
    pub task: Option<Task>,
}

/// A piece of work that the reader handed to the agent.
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Task {
    pub kind: TaskKind,
    /// The change. Empty on a fix of the series.
    pub key: String,
    /// The thread, on an answer.
    pub id: Option<String>,
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum TaskKind {
    Fix,
    Answer,
}

/// How long after a wait that answered an agent still counts as listening.
/// A `qreview wait` that runs out of time returns, and the agent starts the
/// next one on its next turn.
const LISTENS_FOR: Duration = Duration::from_secs(30);

/// How long after a wait that it hung up on an agent still counts as
/// listening. Its `qreview wait` stopped: only one that the agent starts
/// again at once is not leaving.
const HUNG_UP_FOR: Duration = Duration::from_secs(5);

/// How long an ask keeps the agent present while it answers. A long answer
/// is not leaving.
const ANSWERS_FOR: Duration = Duration::from_secs(10 * 60);

/// How long a fix counts as under way with no word of the agent. A fix
/// takes minutes with no wait open; past this, the agent is taken for gone.
const WORKS_FOR: Duration = Duration::from_secs(30 * 60);

/// How long no page may wait before the page counts as closed. A reload
/// opens a new wait within a second.
const PAGE_GONE_AFTER: Duration = Duration::from_secs(3);

#[derive(Default)]
pub struct Events {
    log: Mutex<Log>,
    notify: Notify,
    /// The server stops. A page always holds a wait open, and a stop must
    /// not sit behind it.
    closed: std::sync::atomic::AtomicBool,
    agent: Mutex<Listening>,
    pages: Mutex<Pages>,
}

/// The pages of the review that hold a wait open.
#[derive(Default)]
struct Pages {
    open: usize,
    ended: Option<std::time::Instant>,
}

/// Held while a page waits. A guard, because the reader closes the tab.
pub struct PageWaits<'a>(&'a Events);

impl Drop for PageWaits<'_> {
    fn drop(&mut self) {
        let mut pages = self.0.pages.lock().expect("the page state is poisoned");
        pages.open = pages.open.saturating_sub(1);
        pages.ended = Some(std::time::Instant::now());
    }
}

#[derive(Default)]
struct Listening {
    open: usize,
    ended: Option<std::time::Instant>,
    /// How long the agent counts as listening after `ended`.
    grace: Duration,
    /// The agent wrote since its last wait opened.
    wrote: bool,
    /// When the reader asked for a fix that the agent has not finished.
    fixing: Option<std::time::Instant>,
    /// The change of that fix.
    fix_of: String,
    /// When the reader asked the agent about a thread, while it listened.
    answering: Option<std::time::Instant>,
    /// The change and the thread of that question.
    answer_to: (String, String),
    /// An agent waited once on this server.
    seen: bool,
}

/// Held while an agent waits. A guard, because the agent can hang up.
pub struct AgentWaits<'a> {
    events: &'a Events,
    answered: bool,
}

impl AgentWaits<'_> {
    /// The wait got its answer. A guard dropped before this hung up.
    pub fn answered(&mut self) {
        self.answered = true;
    }
}

impl Drop for AgentWaits<'_> {
    fn drop(&mut self) {
        let mut agent = self
            .events
            .agent
            .lock()
            .expect("the agent state is poisoned");
        agent.open = agent.open.saturating_sub(1);
        agent.ended = Some(std::time::Instant::now());
        agent.grace = if self.answered {
            LISTENS_FOR
        } else {
            HUNG_UP_FOR
        };
        let hung_up = !self.answered && agent.open == 0;
        drop(agent);
        if hung_up {
            // The waits of the pages answer when the grace runs out, and
            // they must learn of it now to count from here.
            self.events.notify.notify_waiters();
        }
    }
}

#[derive(Default)]
struct Log {
    /// The number of the last event, 0 before the first.
    last: u64,
    kept: VecDeque<Event>,
}

impl Events {
    /// Mark an agent as waiting until the guard is dropped.
    ///
    /// A wait that opens after the agent wrote ends its work: the agent goes
    /// back to waiting once it is done, so this says so with no command.
    pub fn agent_waits(&self) -> AgentWaits<'_> {
        let (ended, arrived) = {
            let mut agent = self.agent.lock().expect("the agent state is poisoned");
            let arrived = !listens(&agent);
            agent.open += 1;
            agent.seen = true;
            agent.fixing = None;
            agent.answering = None;
            (std::mem::take(&mut agent.wrote), arrived)
        };
        if ended {
            self.push(Kind::Idle, Author::Agent, "", None);
        } else if arrived {
            // No event, but the page shows whether an agent listens, and its
            // wait would hold the news for up to 30 seconds.
            self.notify.notify_waiters();
        }
        AgentWaits {
            events: self,
            answered: false,
        }
    }

    /// Mark a page as waiting until the guard is dropped.
    pub fn page_waits(&self) -> PageWaits<'_> {
        self.pages.lock().expect("the page state is poisoned").open += 1;
        PageWaits(self)
    }

    /// Whether a page of the review is open. None before the first one.
    pub fn page_open(&self) -> Option<bool> {
        self.page_open_within(PAGE_GONE_AFTER)
    }

    fn page_open_within(&self, grace: Duration) -> Option<bool> {
        let pages = self.pages.lock().expect("the page state is poisoned");
        if pages.open > 0 {
            return Some(true);
        }
        pages.ended.map(|at| at.elapsed() < grace)
    }

    pub fn agent_listens(&self) -> bool {
        listens(&self.agent.lock().expect("the agent state is poisoned"))
    }

    /// An agent listened, and listens no more.
    pub fn agent_left(&self) -> bool {
        let agent = self.agent.lock().expect("the agent state is poisoned");
        agent.seen && !listens(&agent)
    }

    /// How long until an agent with no wait open stops listening, when
    /// only the grace of its last wait keeps it.
    fn agent_gone_in(&self) -> Option<Duration> {
        let agent = self.agent.lock().expect("the agent state is poisoned");
        if agent.open > 0 || answering(&agent) || working(&agent) {
            return None;
        }
        let ended = agent.ended?;
        agent
            .grace
            .checked_sub(ended.elapsed())
            .filter(|left| !left.is_zero())
    }

    pub fn agent_works(&self) -> bool {
        working(&self.agent.lock().expect("the agent state is poisoned"))
    }

    /// What the agent does now. A fix holds every thread of the change, so
    /// it comes before a question asked during it.
    pub fn agent_task(&self) -> Option<Task> {
        let agent = self.agent.lock().expect("the agent state is poisoned");
        if working(&agent) {
            return Some(Task {
                kind: TaskKind::Fix,
                key: agent.fix_of.clone(),
                id: None,
            });
        }
        answering(&agent).then(|| Task {
            kind: TaskKind::Answer,
            key: agent.answer_to.0.clone(),
            id: Some(agent.answer_to.1.clone()),
        })
    }

    pub fn push(&self, kind: Kind, author: Author, key: &str, id: Option<&str>) {
        self.push_with(kind, author, key, id, None);
    }

    pub fn push_migrated(&self, key: &str, versions: Versions<'_>) {
        self.push_with(Kind::Migrated, Author::Agent, key, None, Some(versions));
    }

    fn push_with(
        &self,
        kind: Kind,
        author: Author,
        key: &str,
        id: Option<&str>,
        versions: Option<Versions<'_>>,
    ) {
        {
            let mut agent = self.agent.lock().expect("the agent state is poisoned");
            if kind == Kind::Fix {
                agent.fixing = Some(std::time::Instant::now());
                agent.fix_of = key.to_owned();
            }
            if kind == Kind::Ask && listens(&agent) {
                agent.answering = Some(std::time::Instant::now());
                agent.answer_to = (key.to_owned(), id.unwrap_or_default().to_owned());
            }
            if author == Author::Agent && kind != Kind::Idle {
                agent.wrote = true;
            }
        }
        {
            let mut log = self.log.lock().expect("the event log is poisoned");
            log.last += 1;
            let event = Event {
                seq: log.last,
                kind,
                author,
                key: key.to_owned(),
                id: id.map(str::to_owned),
                from: versions.as_ref().map(|v| v.from.to_owned()),
                to: versions.as_ref().map(|v| v.to.to_owned()),
                from_key: versions.as_ref().map(|v| v.from_key.to_owned()),
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
        // The page shows the agent as gone once its grace runs out, with no
        // event to wake it then.
        let wait = self.agent_gone_in().map_or(wait, |gone| wait.min(gone));
        let _ = tokio::time::timeout(wait, notified).await;

        self.since(after)
    }

    fn since(&self, after: u64) -> Batch {
        let working = self.agent_works();
        let agent = self.agent_listens();
        let left = self.agent_left();
        let task = self.agent_task();
        let log = self.log.lock().expect("the event log is poisoned");
        let next = log.last;

        if after == u64::MAX {
            return Batch {
                events: Vec::new(),
                next,
                reset: false,
                agent,
                working,
                left,
                task: task.clone(),
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
                agent,
                working,
                left,
                task: task.clone(),
            };
        }
        Batch {
            events: log.kept.iter().filter(|e| e.seq > after).cloned().collect(),
            next,
            reset: false,
            agent,
            working,
            left,
            task,
        }
    }
}

fn listens(agent: &Listening) -> bool {
    agent.open > 0
        || agent.ended.is_some_and(|at| at.elapsed() < agent.grace)
        || answering(agent)
        || working(agent)
}

fn answering(agent: &Listening) -> bool {
    agent.answering.is_some_and(|at| at.elapsed() < ANSWERS_FOR)
}

fn working(agent: &Listening) -> bool {
    agent.fixing.is_some_and(|at| at.elapsed() < WORKS_FOR)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;

    const SHORT: Duration = Duration::from_millis(20);

    #[test]
    fn a_page_counts_as_closed_once_no_wait_holds_it() {
        let events = Events::default();
        assert_eq!(events.page_open_within(Duration::ZERO), None, "no page yet");

        let first = events.page_waits();
        let second = events.page_waits();
        drop(first);
        assert_eq!(events.page_open_within(Duration::ZERO), Some(true));

        drop(second);
        assert_eq!(events.page_open_within(Duration::ZERO), Some(false));
        assert_eq!(
            events.page_open_within(Duration::from_secs(60)),
            Some(true),
            "a reload is not a close"
        );
    }

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
    async fn an_agent_listens_while_it_waits_and_a_moment_after() {
        let events = Events::default();
        assert!(!events.agent_listens());

        let mut waiting = events.agent_waits();
        assert!(events.agent_listens());
        waiting.answered();
        drop(waiting);
        assert!(events.agent_listens(), "it reads before it waits again");

        events.agent.lock().unwrap().ended =
            Some(std::time::Instant::now() - LISTENS_FOR - Duration::from_secs(1));
        assert!(!events.agent_listens());
    }

    #[test]
    fn an_agent_that_hangs_up_leaves_sooner_than_one_that_was_answered() {
        let ago = std::time::Instant::now() - HUNG_UP_FOR - Duration::from_millis(1);

        let events = Events::default();
        drop(events.agent_waits());
        assert!(events.agent_listens(), "a wait started again at once");
        events.agent.lock().unwrap().ended = Some(ago);
        assert!(!events.agent_listens(), "its `qreview wait` stopped");

        let events = Events::default();
        let mut waiting = events.agent_waits();
        waiting.answered();
        drop(waiting);
        events.agent.lock().unwrap().ended = Some(ago);
        assert!(events.agent_listens(), "it starts the next wait");
    }

    #[tokio::test]
    async fn the_page_hears_that_the_agent_hung_up_once_its_grace_runs_out() {
        let events = Arc::new(Events::default());
        let waiting = events.agent_waits();
        let page = {
            let events = events.clone();
            tokio::spawn(async move { events.after(Some(0), Duration::from_secs(30)).await })
        };
        tokio::time::sleep(SHORT).await;

        drop(waiting);
        let woken = tokio::time::timeout(Duration::from_secs(1), page)
            .await
            .expect("the hang-up wakes the page")
            .unwrap();
        assert!(woken.agent, "the grace still runs");

        events.agent.lock().unwrap().ended =
            Some(std::time::Instant::now() - HUNG_UP_FOR + Duration::from_millis(200));
        let gone = tokio::time::timeout(
            Duration::from_secs(1),
            events.after(Some(0), Duration::from_secs(30)),
        )
        .await
        .expect("the page wait ends with the grace");
        assert!(gone.left);
    }

    #[tokio::test]
    async fn an_agent_that_listened_and_stopped_has_left() {
        let events = Events::default();
        assert!(!events.after(None, SHORT).await.left, "no agent came yet");

        drop(events.agent_waits());
        assert!(!events.after(None, SHORT).await.left);

        events.agent.lock().unwrap().ended =
            Some(std::time::Instant::now() - LISTENS_FOR - Duration::from_secs(1));
        assert!(events.after(None, SHORT).await.left);

        drop(events.agent_waits());
        assert!(!events.after(None, SHORT).await.left, "it came back");
    }

    #[tokio::test]
    async fn an_ask_keeps_the_agent_present_while_it_answers() {
        let events = Events::default();
        drop(events.agent_waits());
        events.push(Kind::Ask, Author::Reader, "I1", Some("c-1"));
        events.agent.lock().unwrap().ended =
            Some(std::time::Instant::now() - LISTENS_FOR - Duration::from_secs(1));

        assert!(events.agent_listens(), "a long answer is not leaving");

        drop(events.agent_waits());
        events.agent.lock().unwrap().ended =
            Some(std::time::Instant::now() - LISTENS_FOR - Duration::from_secs(1));
        assert!(!events.agent_listens(), "the wait ended the answer");
    }

    #[tokio::test]
    async fn an_agent_that_arrives_wakes_the_page() {
        let events = Arc::new(Events::default());
        let page = {
            let events = events.clone();
            tokio::spawn(async move { events.after(Some(0), Duration::from_secs(30)).await })
        };
        tokio::time::sleep(SHORT).await;

        let _waits = events.agent_waits();

        let batch = tokio::time::timeout(Duration::from_secs(1), page)
            .await
            .expect("the page heard of the agent at once")
            .unwrap();
        assert!(batch.agent);
    }

    #[tokio::test]
    async fn the_task_names_the_fix_first_then_the_question() {
        let events = Events::default();
        drop(events.agent_waits());
        assert_eq!(events.agent_task(), None);

        events.push(Kind::Ask, Author::Reader, "I1", Some("c-1"));
        assert_eq!(
            events.agent_task(),
            Some(Task {
                kind: TaskKind::Answer,
                key: "I1".to_owned(),
                id: Some("c-1".to_owned()),
            })
        );

        events.push(Kind::Fix, Author::Reader, "I2", None);
        let task = events.after(None, SHORT).await.task.unwrap();
        assert_eq!((task.kind, task.key.as_str()), (TaskKind::Fix, "I2"));

        drop(events.agent_waits());
        assert_eq!(events.agent_task(), None, "a wait ends the work");
    }

    #[tokio::test]
    async fn an_ask_with_no_agent_keeps_no_agent_present() {
        let events = Events::default();
        events.push(Kind::Ask, Author::Reader, "I1", Some("c-1"));

        assert!(!events.agent_listens());
    }

    #[tokio::test]
    async fn a_wait_after_the_agent_wrote_says_it_is_idle() {
        let events = Events::default();
        drop(events.agent_waits());
        assert_eq!(
            events.after(None, SHORT).await.next,
            0,
            "nothing written yet"
        );

        events.push(Kind::Reply, Author::Agent, "I1", Some("c-1"));
        events.push(Kind::Comment, Author::Reader, "I1", Some("c-2"));
        drop(events.agent_waits());
        drop(events.agent_waits());

        let batch = events.after(Some(2), SHORT).await;
        assert_eq!(batch.events.len(), 1, "one idle, not one per wait");
        assert_eq!(batch.events[0].kind, Kind::Idle);
    }

    #[tokio::test]
    async fn a_fix_keeps_the_agent_present_until_it_waits_again() {
        let events = Events::default();
        drop(events.agent_waits());
        events.push(Kind::Fix, Author::Reader, "I1", None);
        events.agent.lock().unwrap().ended =
            Some(std::time::Instant::now() - LISTENS_FOR - Duration::from_secs(1));

        assert!(events.agent_works());
        assert!(events.agent_listens(), "a long fix is not leaving");

        let waits = events.agent_waits();
        assert!(!events.agent_works());
        drop(waits);

        events.push(Kind::Fix, Author::Reader, "I1", None);
        events.agent.lock().unwrap().fixing =
            Some(std::time::Instant::now() - WORKS_FOR - Duration::from_secs(1));
        assert!(!events.agent_works(), "an agent that died is gone at last");
    }

    #[tokio::test]
    async fn a_migrated_event_names_both_versions() {
        let events = Events::default();
        events.push_migrated(
            "sha-b",
            Versions {
                from: "a",
                to: "b",
                from_key: "sha-a",
            },
        );

        let json = serde_json::to_value(&events.after(Some(0), SHORT).await.events[0]).unwrap();
        assert_eq!(json["from"], "a");
        assert_eq!(json["to"], "b");
        assert_eq!(json["fromKey"], "sha-a");
        let other = Events::default();
        other.push(Kind::Reply, Author::Agent, "I1", None);
        let json = serde_json::to_value(&other.after(Some(0), SHORT).await.events[0]).unwrap();
        assert!(json.get("from").is_none());
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
