// What the top bar says the agent does.
//
// "Agent working" says that something runs, not what. The reader handed the
// work over and went elsewhere, so the bar names it: the change of a fix, or
// the start of the remark that a question is about.

import type { AgentTask, Comment } from '@/api/types';

/// How much of a remark the bar shows.
const SHOWN = 40;

export interface Activity {
  text: string;
  /// The whole remark or subject, for the tooltip.
  title: string;
}

/// Name the task of the agent. `subjectOf` and `commentOf` read what the
/// page holds; a change or a thread it does not hold is named in general.
export function describe(
  task: AgentTask,
  subjectOf: (key: string) => string | undefined,
  commentOf: (key: string, id: string) => Comment | undefined,
): Activity {
  if (task.kind === 'fix') {
    // A fix of the series names no change.
    if (task.key === '') {
      return { text: 'Agent fixing the series…', title: '' };
    }
    const subject = subjectOf(task.key);
    return subject
      ? { text: `Agent fixing “${clip(subject)}”…`, title: subject }
      : { text: 'Agent fixing a change…', title: '' };
  }
  const remark = task.id ? commentOf(task.key, task.id) : undefined;
  return remark
    ? { text: `Agent answering “${clip(remark.body)}”…`, title: remark.body }
    : { text: 'Agent answering a thread…', title: '' };
}

/// The first line of a text, cut to fit the bar.
function clip(text: string): string {
  const line = text.split('\n')[0]?.trim() ?? '';
  return line.length > SHOWN ? `${line.slice(0, SHOWN - 1)}…` : line;
}
