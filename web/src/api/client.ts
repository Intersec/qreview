// Every call to the server. The token travels as a cookie the first page
// load left behind, so nothing here carries it.

import type {
  ChangeComments,
  ChangeSummary,
  Config,
  Comment,
  EditComment,
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
    throw new ApiError(error?.code ?? 'failed', error?.message ?? response.statusText);
  }
  return response.json() as Promise<T>;
}

export const api = {
  session: () => call<SessionBody>('/api/session'),

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

  posted: (key: string, ps?: number) =>
    call<Posted>(`/api/changes/${encodeURIComponent(key)}/posted${query({ ps: num(ps) })}`),
  comments: (key: string, ps?: number) =>
    call<Review>(`/api/changes/${encodeURIComponent(key)}/comments${query({ ps: num(ps) })}`),

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
