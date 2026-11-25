// @vitest-environment jsdom
//
// What the block of change info shows of the change on Gerrit.

import { mount } from '@vue/test-utils';
import { describe, expect, it } from 'vitest';
import ChangeInfo from './ChangeInfo.vue';
import type { GerritChange, PatchSet } from '@/api/types';

const GERRIT: GerritChange = {
  number: 12321,
  url: 'https://review.example.com/c/myproject/+/12321',
  project: 'myproject',
  branch: 'main',
  status: 'NEW',
  topic: 'net-retry',
  owner: 'Ada',
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
    const text = mount(ChangeInfo, { props: { gerrit: GERRIT, sets: sets(0) } }).text();

    expect(text).toContain('12321');
    expect(text).toContain('Active');
    expect(text).toContain('Ada');
    expect(text).toContain('myproject | main');
    expect(text).toContain('net-retry');
  });

  it('counts the remarks of every version, and names where they are', () => {
    const text = mount(ChangeInfo, { props: { gerrit: GERRIT, sets: sets(2, 0, 1) } }).text();

    expect(text).toContain('3 on Gerrit, on Patchset 1, Patchset 3');
  });

  it('shows nothing for a change the server does not know', () => {
    expect(mount(ChangeInfo, { props: { gerrit: null, sets: [] } }).html()).not.toContain('dl');
  });
});
