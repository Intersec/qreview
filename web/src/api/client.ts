// Every call to the server. The token travels as a cookie the first page
// load left behind, so nothing here carries it.

import type {
  ChangeComments,
  ChangeSummary,
  Config,
  DraftPlace,
  Comment,
  EditComment,
  EventBatch,
  FileDiff,
  FileEntry,
  MergeListItem,
  NewComment,
  PatchSet,
  PatchSets,
  Posted,
  Release,
  Review,
  Row,
  Series,
  SessionBody,
} from './types';

export class ApiError extends Error {
  constructor(
    public code: string,
    message: string,
  ) {
    super(message);
  }
}

async function call<T>(path: string, init?: RequestInit): Promise<T> {
  const response = await fetch(path, {
    credentials: 'same-origin',
    ...init,
  });

  if (!response.ok) {
    const body = await response.json().catch(() => null);
    const error = body?.error;
    // The token guard answers in plain text, before any route.
    const code = response.status === 401 ? 'unauthorized' : 'failed';
    throw new ApiError(error?.code ?? code, error?.message ?? response.statusText);
  }
  return response.json() as Promise<T>;
}

/// True when the server that served this page is gone for good.
///
/// The server is on the loopback, so a request that reaches nothing means
/// it stopped. A refused token means another server took its port, and the
/// cookie of this page opens nothing there.
export function serverGone(error: unknown): boolean {
  return error instanceof TypeError || (error instanceof ApiError && error.code === 'unauthorized');
}

export const api = {
  session: () => call<SessionBody>('/api/session'),

  /// Read the repository again. The series comes back as git holds it now.
  refresh: () => call<SessionBody>('/api/session/refresh', { method: 'POST' }),

  saveConfig: (patch: object) =>
    call<Config>('/api/config', {
      method: 'PUT',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(patch),
    }),

  extend: (count: number) =>
    call<Series>('/api/series/extend', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ count }),
    }),

  files: (key: string, ps?: number, base?: string, ignoreWs = false) =>
    call<FileEntry[]>(
      `/api/changes/${encodeURIComponent(key)}/files${query({
        ps: num(ps),
        base,
        ws: ignoreWs ? 'ignore' : undefined,
      })}`,
    ),

  diff: (key: string, file: string, ps?: number, base?: string, ignoreWs = false) =>
    call<FileDiff>(
      `/api/changes/${encodeURIComponent(key)}/diff${query({
        file,
        ps: num(ps),
        base,
        ws: ignoreWs ? 'ignore' : undefined,
      })}`,
    ),

  lines: (key: string, file: string, from: number, to: number, ps?: number) =>
    call<Row[]>(
      `/api/changes/${encodeURIComponent(key)}/lines${query({
        file,
        from: String(from),
        to: String(to),
        ps: num(ps),
      })}`,
    ),

  patchSets: (key: string) => call<PatchSets>(`/api/changes/${encodeURIComponent(key)}/patchsets`),

  fetchPatchSet: (key: string, number: number) =>
    call<PatchSet>(`/api/changes/${encodeURIComponent(key)}/patchsets/${number}/fetch`, {
      method: 'POST',
    }),

  mergeList: (key: string) =>
    call<MergeListItem[]>(`/api/changes/${encodeURIComponent(key)}/mergelist`),

  /// The review as Markdown. Not JSON: it is made to be pasted.
  exportText: async (key?: string, all = false): Promise<string> => {
    const params = new URLSearchParams();
    if (key) {
      params.set('scope', 'change');
      params.set('key', key);
    }
    if (all) {
      params.set('all', 'true');
    }
    const response = await fetch(`/api/export?${params.toString()}`, {
      credentials: 'same-origin',
    });
    if (!response.ok) {
      throw new ApiError('failed', response.statusText);
    }
    return response.text();
  },

  markChange: (key: string, reviewed: boolean) =>
    call<ChangeSummary>(`/api/changes/${encodeURIComponent(key)}`, {
      method: 'PATCH',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ reviewed }),
    }),

  /// Is a newer qreview out? A failure to ask is not an error.
  update: () => call<Release>('/api/update'),

  /// Every comment of the session, in the order a review reads them.
  allComments: () => call<ChangeComments[]>('/api/comments'),

  /// The writes to the store after a number, held until one comes.
  /// A wait names the page, so the server says in its terminal when the
  /// page closes.
  events: (after?: number) =>
    call<EventBatch>(
      `/api/events${query({ after: num(after), who: after === undefined ? undefined : 'reader' })}`,
    ),

  /// Where places of one version stand in the next one. Null for a file the
  /// next one deleted.
  map: (from: string, to: string, places: DraftPlace[]) =>
    call<{ places: (DraftPlace | null)[] }>('/api/map', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ from, to, places }),
    }),

  posted: (key: string, ps?: number, base?: string) =>
    call<Posted>(`/api/changes/${encodeURIComponent(key)}/posted${query({ ps: num(ps), base })}`),
  comments: (key: string, ps?: number, base?: string) =>
    call<Review>(`/api/changes/${encodeURIComponent(key)}/comments${query({ ps: num(ps), base })}`),

  addComment: (key: string, comment: NewComment) =>
    call<Comment>(`/api/changes/${encodeURIComponent(key)}/comments`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(comment),
    }),

  editComment: (key: string, id: string, edit: EditComment) =>
    call<Comment>(`/api/changes/${encodeURIComponent(key)}/comments/${encodeURIComponent(id)}`, {
      method: 'PATCH',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(edit),
    }),

  /// Ask the agent to answer one thread.
  ask: (key: string, id: string) =>
    fetch(`/api/changes/${encodeURIComponent(key)}/comments/${encodeURIComponent(id)}/ask`, {
      method: 'POST',
      credentials: 'same-origin',
    }).then(async (response) => {
      if (!response.ok) {
        const body = await response.json().catch(() => null);
        throw new ApiError(body?.error?.code ?? 'failed', body?.error?.message ?? 'failed');
      }
    }),

  /// Ask the agent to fix the change, or the whole series with no key.
  fix: (key?: string) =>
    fetch(key ? `/api/changes/${encodeURIComponent(key)}/fix` : '/api/series/fix', {
      method: 'POST',
      credentials: 'same-origin',
    }).then(async (response) => {
      if (!response.ok) {
        const body = await response.json().catch(() => null);
        throw new ApiError(body?.error?.code ?? 'failed', body?.error?.message ?? 'failed');
      }
    }),

  clean: (key: string) =>
    call<{ deleted: number }>(`/api/changes/${encodeURIComponent(key)}/clean`, {
      method: 'POST',
    }),

  deleteComment: (key: string, id: string) =>
    call<{ deleted: number }>(
      `/api/changes/${encodeURIComponent(key)}/comments/${encodeURIComponent(id)}`,
      { method: 'DELETE' },
    ),
};

function num(value: number | undefined): string | undefined {
  return value === undefined ? undefined : String(value);
}

function query(params: Record<string, string | undefined>): string {
  const pairs = Object.entries(params).filter(([, value]) => value !== undefined);
  if (pairs.length === 0) {
    return '';
  }
  return `?${new URLSearchParams(pairs as [string, string][]).toString()}`;
}
