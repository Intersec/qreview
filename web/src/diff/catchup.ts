// What the agent wrote and the page has not shown yet.
//
// A fix is a burst of writes: replies, an amend, one migrate per commit a
// rebase rewrote. A reload at each one made the page flicker for minutes.
// While the agent works, the page notes what changed, and catches up once
// when the agent is done.

import type { StoreEvent } from '@/api/types';

/// How often a page that holds the work of the agent shows it anyway. A long
/// fix still shows where it stands, without a reload at every step.
export const CATCH_UP_EVERY = 10 * 60 * 1000;

export interface Actions {
  /// The change on the screen.
  current(): string | null;
  /// Move the unsaved remarks of the version a migrate replaced.
  followDrafts(event: StoreEvent): Promise<void>;
  /// Read the whole series again.
  refresh(): Promise<void>;
  /// Read the change on the screen again.
  reload(): Promise<void>;
  /// Read the counts of the other changes again.
  readWritten(): Promise<void>;
  /// The page shows what the agent did: a fix it ended, or an answer.
  told(what: 'fix' | 'answer'): void;
}

export class Catchup {
  private migrated: StoreEvent[] = [];
  private series = false;
  private change = false;
  private elsewhere = false;
  private since: number;

  /// `working`: a fix runs when the page starts listening.
  constructor(
    private readonly act: Actions,
    private working = false,
    private readonly now: () => number = Date.now,
  ) {
    this.since = now();
  }

  /// Take the events of one answer, and show them unless the agent works.
  async settle(events: StoreEvent[], working: boolean) {
    const ended = this.working && !working;
    this.working = working;
    // An answer outside a fix: during one, the end of the fix tells it all.
    const answered =
      !working &&
      events.some((e) => e.author === 'agent' && (e.kind === 'reply' || e.kind === 'comment'));

    this.note(events);
    if (!working || this.now() - this.since > CATCH_UP_EVERY) {
      await this.apply();
    }
    if (ended) {
      this.act.told('fix');
    } else if (answered) {
      this.act.told('answer');
    }
  }

  private note(events: StoreEvent[]) {
    for (const event of events) {
      if (event.author !== 'agent' || event.kind === 'idle') {
        continue;
      }
      if (event.kind === 'migrated') {
        this.migrated.push(event);
      }
      // A migrate moves threads to a new commit, and a change without a
      // Change-Id to a new key: the whole series is read again.
      if (event.kind === 'refresh' || event.kind === 'migrated') {
        this.series = true;
      } else if (event.key === this.act.current()) {
        this.change = true;
      } else {
        this.elsewhere = true;
      }
    }
  }

  private async apply() {
    const { migrated, series, change, elsewhere } = this;
    this.migrated = [];
    this.series = this.change = this.elsewhere = false;
    this.since = this.now();

    // The drafts first, so the diff read next opens them where they now
    // belong.
    for (const event of migrated) {
      await this.act.followDrafts(event);
    }
    if (series) {
      await this.act.refresh();
    } else if (change) {
      await this.act.reload();
    } else if (elsewhere) {
      await this.act.readWritten();
    }
  }
}
