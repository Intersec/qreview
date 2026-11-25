// @vitest-environment jsdom
//
// What the block of change info shows of the change on Gerrit.

import { mount } from '@vue/test-utils';
import { describe, expect, it } from 'vitest';
import ChangeBox from './ChangeBox.vue';
import type { ChangeSummary, GerritChange, PatchSet } from '@/api/types';

const GERRIT: GerritChange = {
  number: 12321,
  url: 'https://review.example.com/c/myproject/+/12321',
  project: 'myproject',
  branch: 'main',
  status: 'NEW',
  topic: 'net-retry',
  owner: 'Ada',
};

const CHANGE: ChangeSummary = {
  key: 'I1',
  changeId: 'I1',
  subject: 'net: retry the read',
  author: 'Ada',
  commit: 'a'.repeat(40),
  parents: ['b'.repeat(40)],
  patchSetCount: 1,
  commentCount: 0,
  reviewed: false,
  isMerge: false,
  worktree: false,
};

function sets(...posted: number[]): PatchSet[] {
  return posted.map((count, i) => ({
    number: i + 1,
    commit: `${i + 1}`.repeat(40),
    parent: null,
    origin: 'gerrit',
    createdAt: '',
    subject: '',
    available: true,
    posted: count,
  }));
}

describe('the change info', () => {
  it('shows what Gerrit knows of the change', () => {
    const text = mount(ChangeBox, {
      props: { gerrit: GERRIT, sets: sets(0), change: CHANGE },
    }).text();

    expect(text).toContain('12321');
    expect(text).toContain('Active');
    expect(text).toContain('Ada');
    expect(text).toContain('myproject | main');
    expect(text).toContain('net-retry');
  });

  it('counts the remarks of every version, and names where they are', () => {
    const text = mount(ChangeBox, {
      props: { gerrit: GERRIT, sets: sets(2, 0, 1), change: CHANGE },
    }).text();

    expect(text).toContain('3 on Gerrit, on Patchset 1, Patchset 3');
  });

  it('says so for a change the server does not know', () => {
    const text = mount(ChangeBox, { props: { gerrit: null, sets: [], change: CHANGE } }).text();

    expect(text).toContain('not on the server');
    expect(text).toContain('Ada');
    expect(text).toContain('aaaaaaaaaaaa');
  });
});
