// Every call to the server. The token travels as a cookie the first page
// load left behind, so nothing here carries it.

import type { FileDiff, FileEntry, MergeListItem, Series, SessionBody } from './types';

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

  extend: (count: number) =>
    call<Series>('/api/series/extend', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ count }),
    }),

  files: (key: string, base?: string) =>
    call<FileEntry[]>(`/api/changes/${encodeURIComponent(key)}/files${query({ base })}`),

  diff: (key: string, file: string, base?: string) =>
    call<FileDiff>(`/api/changes/${encodeURIComponent(key)}/diff${query({ file, base })}`),

  mergeList: (key: string) =>
    call<MergeListItem[]>(`/api/changes/${encodeURIComponent(key)}/mergelist`),
};

function query(params: Record<string, string | undefined>): string {
  const pairs = Object.entries(params).filter(([, value]) => value !== undefined);
  if (pairs.length === 0) {
    return '';
  }
  return `?${new URLSearchParams(pairs as [string, string][]).toString()}`;
}
