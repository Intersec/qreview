// The wire contract. The Rust structs in crates/qreview/src/model.rs are the
// source, and one snapshot test per route holds them to this shape.

export type BoundaryKind = 'merge' | 'tag' | 'base' | 'guess' | 'batch' | 'root';

export interface RepoInfo {
  root: string;
  remote: string | null;
  id: string;
  /// What the repository is called, for a person to read.
  name: string;
}

export interface ParentInfo {
  commit: string;
  name: string;
  remote: boolean;
}

export interface MergeInfo {
  subject: string;
  parents: ParentInfo[];
}

export interface Boundary {
  kind: BoundaryKind;
  commit: string | null;
  /// The subject of that commit. Absent at the root, which has no commit.
  subject: string | null;
  /// How many commits still stand between here and the base, when a base is
  /// known and the batch stopped short of it.
  remaining: number | null;
  reason: string;
  guessed: boolean;
  merge: MergeInfo | null;
}

export interface ChangeSummary {
  key: string;
  changeId: string | null;
  subject: string;
  author: string;
  commit: string;
  /// The parents of the commit, in order. Two or more mean a merge.
  parents: string[];
  patchSetCount: number;
  commentCount: number;
  reviewed: boolean;
  isMerge: boolean;
  /// The work that is not committed yet, not a commit of the history.
  worktree: boolean;
  /// The lines the commit adds and removes against its first parent.
  /// Absent on a merge, and when git could not count them.
  lines?: LineCount | null;
  /// False on the change of a colleague, which the reader does not fix.
  /// See section 5.9 of the design.
  own?: boolean;
}

export interface LineCount {
  added: number;
  removed: number;
}

export interface Series {
  repo: RepoInfo;
  head: string;
  oldest: string;
  changes: ChangeSummary[];
  boundary: Boundary;
}

export type FileStatus = 'added' | 'modified' | 'deleted' | 'renamed' | 'copied';

export interface FileEntry {
  path: string;
  oldPath: string | null;
  status: FileStatus;
  language: string;
  binary: boolean;
  added: number;
  removed: number;
}

export type RowKind = 'context' | 'add' | 'remove';

/** Offsets are UTF-16 units, which is what a browser slices with. */
export interface Span {
  start: number;
  end: number;
  cls: string;
}

export interface WordSpan {
  start: number;
  end: number;
}

export interface Row {
  kind: RowKind;
  oldLine: number | null;
  newLine: number | null;
  text: string;
  noNewline?: boolean;
  tokens?: Span[];
  words?: WordSpan[];
  /// Between two patch sets, the rebase brought this row rather than the
  /// work of the change.
  rebase?: boolean;
}

export interface Hunk {
  oldStart: number;
  oldLines: number;
  newStart: number;
  newLines: number;
  header: string;
  rows: Row[];
}

export interface FileDiff extends FileEntry {
  hunks: Hunk[];
  /// How many lines the new side has. Absent when the file could not be read.
  lineCount?: number | null;
  /// The server can render the file, so it can be read rendered.
  rendered?: boolean;
}

/// A Markdown file, as HTML. Each leaf block carries `data-lines`, and the
/// marks of the diff are `data-change` and `data-old`.
export interface Rendered {
  html: string;
}

export interface Ui {
  /// `unified` or `side-by-side`.
  diff: string;
  /// `system`, `light` or `dark`.
  theme: string;
}

export interface DiffPrefs {
  context: number;
  wrap: boolean;
  ignoreWhitespace: boolean;
  tabWidth: number;
  fontSize: number;
  syntax: boolean;
}

export interface Config {
  languages: Record<string, string>;
  gerrit: { enabled: boolean; branch: string | null };
  series: {
    maxCommits: number;
    guessMax: number;
    batchSize: number;
    integrationBranch: string | null;
  };
  ui: Ui;
  diff: DiffPrefs;
}

export interface SessionBody {
  version: string;
  /// The release and the commit under it, `0.5.4 (1a2b3c4de)`.
  build: string;
  series: Series;
  config: Config;
}

export interface MergeListItem {
  commit: string;
  subject: string;
  author: string;
  date: string;
}

export type Scope = 'line' | 'range' | 'file' | 'change';
export type Side = 'old' | 'new';

export interface Anchor {
  file: string;
  side: Side;
  startLine: number | null;
  endLine: number | null;
  /// The character range inside the two lines, in UTF-16 units. Null when
  /// the comment covers whole lines.
  startChar: number | null;
  endChar: number | null;
  blob: string | null;
  lineHash: string | null;
  context: string[];
}

export interface Comment {
  id: string;
  patchSet: number;
  /// The commit it was written against. Empty on a remark written by a
  /// qreview older than storage format 3.
  commit: string;
  createdAt: string;
  updatedAt: string;
  scope: Scope;
  body: string;
  anchor: Anchor | null;
  author: Author;
  /// The name that the agent gave, on a comment of the agent. A draft of
  /// Gerrit starts with it.
  agentName?: string;
  /// The remark that opens the thread, on a reply.
  parent: string | null;
  /// The thread is done. Only on the remark that opens it.
  done: boolean;
  /// A reply of the agent that waits for the reader.
  blocked: boolean;
  /// Set when the remark goes to Gerrit. Null on a remark that stays here.
  gerrit: GerritMark | null;
}

/// A remark bound for Gerrit, and how far it went.
export interface GerritMark {
  /// `staged` waits for Send; `sent` is a draft on Gerrit.
  state: 'staged' | 'sent';
  /// The patch set of Gerrit that the draft goes to.
  patchSet: number;
  resolve: boolean;
  id: string | null;
  sentAt: string | null;
}

/// What the REST API of Gerrit allows this run.
export interface GerritStatus {
  rest: 'ok' | 'anonymous' | 'refused' | 'unreachable' | 'off';
  /// Why it does not allow more, for the reader.
  reason: string | null;
  base: string | null;
}

/// What a send did.
export interface Sent {
  sent: string[];
  failed: { id: string; error: string }[];
}

/// What staging a remark for Gerrit asks for.
export interface Stage {
  resolve?: boolean;
}

export type Author = 'reader' | 'agent';

export interface StoreEvent {
  seq: number;
  kind:
    | 'comment'
    | 'reply'
    | 'edited'
    | 'done'
    | 'deleted'
    | 'refresh'
    | 'ask'
    | 'migrated'
    | 'fix'
    | 'clean'
    | 'idle'
    | 'sent';
  author: Author;
  key: string;
  id: string | null;
  /// On a `migrated` event: the old version, the new one, and the change the
  /// old one was kept under.
  from?: string;
  to?: string;
  fromKey?: string;
}

/// A place in a file, as an unfinished remark is kept.
export interface DraftPlace {
  file: string;
  side: Side;
  line: number | null;
}

export interface EventBatch {
  events: StoreEvent[];
  next: number;
  /// The server restarted, or the events fell out of its memory.
  reset: boolean;
  /// An agent listens, so Fix has someone to ask.
  agent: boolean;
  /// The agent works on a fix, and has not waited again since.
  working: boolean;
  /// An agent listened to this server, and listens no more.
  left: boolean;
  /// What the agent does now.
  task: AgentTask | null;
  /// A digest of `agent`, `working`, `left` and `task`. The next wait sends
  /// it back, and answers at once if the digest changed since.
  state: string;
}

/// A piece of work that the reader handed to the agent.
export interface AgentTask {
  kind: 'fix' | 'answer';
  key: string;
  /// The thread, on an answer.
  id: string | null;
}

export interface ChangeFile {
  version: number;
  key: string;
  subject: string;
  comments: Comment[];
}

export interface NewComment {
  scope: Scope;
  file?: string;
  side?: Side;
  startLine?: number;
  endLine?: number;
  startChar?: number;
  endChar?: number;
  body: string;
  /// The comment this one answers.
  parent?: string;
  /// On a reply, check or clear the Done box of the thread with it.
  done?: boolean;
  /// Ask the agent to answer the thread once the comment is saved.
  ask?: boolean;
  /// Stage the remark for Gerrit.
  gerrit?: Stage;
}

export interface EditComment {
  body?: string;
  done?: boolean;
  /// Stage the remark for Gerrit, or, with null, keep it here.
  gerrit?: Stage | null;
}

export type Origin = 'local' | 'prev' | 'gerrit';

export interface GerritChange {
  number: number;
  url: string;
  branch: string;
  status: string;
}

export interface PatchSets {
  sets: PatchSet[];
  gerrit: GerritChange | null;
}

export interface PatchSet {
  number: number;
  commit: string;
  parent: string | null;
  origin: Origin;
  createdAt: string;
  subject: string;
  gerritRef?: string;
  /// False when the commit is not in this clone yet.
  available: boolean;
}

/** Where a comment lands in the patch set being read. */
export interface Placed {
  id: string;
  /// The column it stands in. A Gerrit remark of the version read against
  /// stands on the left.
  side: Side;
  line: number | null;
  /// The last line of the range. It follows the first one.
  endLine: number | null;
  moved: boolean;
  /// The line it spoke of is not in this version. The remark then stands at
  /// the top of the file it was written on.
  lost: boolean;
}

/**
 * A remark already posted on Gerrit. It is edited in Gerrit, never here.
 * Over ssh, the id is made from the place, and the thread, the reply link
 * and the state are missing.
 */
export interface PostedComment {
  id: string;
  patchSet: number;
  author: string;
  body: string;
  file: string;
  line: number | null;
  /// The id of the remark that opens the thread: its own id when it opens it.
  thread: string;
  inReplyTo: string | null;
  unresolved: boolean | null;
  updated: string | null;
}

/** A thread of Gerrit on a change, with the replies written here. */
export interface GerritThread {
  /// The id of the comment that opens it, which `gerrit:<id>` names.
  id: string;
  patchSet: number;
  /// The revision of that patch set.
  commit: string;
  file: string;
  side: Side;
  line: number | null;
  /// Its newest comment on Gerrit waits for an answer.
  unresolved: boolean;
  /// It waits for the reader: unresolved, and its newest comment is of
  /// somebody else.
  waits: boolean;
  /// A reply written here resolves it, staged or sent.
  done: boolean;
  comments: PostedComment[];
  replies: Comment[];
}

/** The threads of Gerrit of one change. */
export interface ChangeThreads {
  key: string;
  /// The page of the change on Gerrit.
  url: string;
  /// Gerrit holds the version that the change carries now.
  pushed: boolean;
  /// The ids of the drafts of the reader. Null without an authorization.
  drafts: string[] | null;
  threads: GerritThread[];
}

/// Where a remark sent to Gerrit is now. See section 5.8 of the design.
export type SentState = 'draft' | 'published' | 'deleted';

/** What Gerrit already holds, placed in the patch set being read. */
export interface Posted {
  comments: PostedComment[];
  placed: Placed[];
}

/** A version of a change that some of its remarks were written on. */
export interface Version {
  commit: string;
  /// The subject it carried. Empty when git no longer has the commit.
  subject: string;
}

/** The comments of one change, in the order a review reads them. */
export interface ChangeComments {
  key: string;
  subject: string;
  commit: string;
  comments: Comment[];
  /// The versions before this one that its remarks were written on, newest
  /// first.
  versions: Version[];
}

/** Whether a newer qreview is out. */
export interface Release {
  latest: string | null;
  url: string | null;
  newer: boolean;
}

export interface Review extends ChangeFile {
  placed: Placed[];
  /// The copies of the threads that left a version on the screen, by the id
  /// of the copy: the thread they became, and the version it went to.
  moved?: Record<string, { id: string; commit: string }>;
}

/** Who wrote the lines of the file on the left of a diff. */
export interface Blame {
  /// Each commit once, in the order its first line comes. Empty when the
  /// left has no history: an added file, the message, an auto-merge.
  commits: BlameCommit[];
}

export interface BlameCommit {
  commit: string;
  author: string;
  email: string;
  /// The author date, ISO 8601 in UTC.
  date: string;
  /// The whole message, from the subject to the last trailer.
  message: string;
  /// The path of the file in that commit. A later rename changes it.
  path: string;
  ranges: BlameRange[];
}

/** Lines of the file on the left that one commit wrote. */
export interface BlameRange {
  start: number;
  /// The last line, included.
  end: number;
  /// Where `start` stands in the commit that wrote it.
  origin: number;
}

/** A slow call of the server in flight, such as a query to Gerrit. */
export interface Waiting {
  what: string;
  /// How long it has run, in milliseconds.
  ms: number;
}

/** What the server waits on now. */
export interface Activity {
  waiting: Waiting[];
}

/** One line of the trace of the server. */
export interface TraceLine {
  n: number;
  /// Seconds since the start of the run.
  at: number;
  /// What the work cost, in milliseconds. Null for an event.
  ms: number | null;
  what: string;
}

/** The lines of the trace after a number. */
export interface TraceLines {
  lines: TraceLine[];
  next: number;
}
