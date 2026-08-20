// The wire contract. The Rust structs in crates/qreview/src/model.rs are the
// source, and one snapshot test per route holds them to this shape.

export type BoundaryKind = 'merge' | 'tag' | 'base' | 'guess' | 'batch' | 'root';

export interface RepoInfo {
  root: string;
  remote: string | null;
  id: string;
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
  patchSetCount: number;
  commentCount: number;
  unresolvedCount: number;
  isMerge: boolean;
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
}

export interface SessionBody {
  version: string;
  series: Series;
}

export interface MergeListItem {
  commit: string;
  subject: string;
  author: string;
  date: string;
}

/** What a merge is read against. Absent means the auto-merge. */
export type MergeBase = 'automerge' | 'parent1' | 'parent2';

export type Scope = 'line' | 'range' | 'file' | 'change';
export type Side = 'old' | 'new';

export interface Anchor {
  file: string;
  side: Side;
  startLine: number | null;
  endLine: number | null;
  blob: string | null;
  lineHash: string | null;
  context: string[];
}

export interface Comment {
  id: string;
  parentId: string | null;
  patchSet: number;
  author: string;
  createdAt: string;
  updatedAt: string;
  scope: Scope;
  resolved: boolean;
  draft: boolean;
  body: string;
  anchor: Anchor | null;
}

export interface ChangeFile {
  version: number;
  key: string;
  subject: string;
  comments: Comment[];
}

export interface NewComment {
  parentId?: string;
  scope: Scope;
  file?: string;
  side?: Side;
  startLine?: number;
  endLine?: number;
  body: string;
  draft?: boolean;
}

export interface EditComment {
  body?: string;
  resolved?: boolean;
  draft?: boolean;
}

export type Origin = 'local' | 'prev' | 'gerrit';

export interface PatchSet {
  number: number;
  commit: string;
  parent: string | null;
  origin: Origin;
  createdAt: string;
  subject: string;
  gerritRef?: string;
}

/** Where a comment lands in the patch set being read. */
export interface Placed {
  id: string;
  line: number | null;
  moved: boolean;
  lost: boolean;
}

export interface Review extends ChangeFile {
  placed: Placed[];
}
