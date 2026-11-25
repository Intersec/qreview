// @vitest-environment jsdom
//
// What the patch set bar says about the Gerrit remarks of each version.

import { mount } from '@vue/test-utils';
import { describe, expect, it } from 'vitest';
import PatchSetBar from './PatchSetBar.vue';
import type { GerritChange, PatchSet } from '@/api/types';

const GERRIT: GerritChange = {
  number: 12321,
  url: 'https://review.example.com/c/myproject/+/12321',
  project: 'myproject',
  branch: 'main',
  status: 'NEW',
  topic: '',
  owner: 'Ada',
};

function set(number: number, over: Partial<PatchSet> = {}): PatchSet {
  return {
    number,
    commit: `${number}`.repeat(40),
    parent: 'p'.repeat(40),
    origin: 'gerrit',
    createdAt: '2026-11-17T16:23:10Z',
    subject: '',
    gerritRef: `refs/changes/21/12321/${number}`,
    available: true,
    posted: 0,
    ...over,
  };
}

/// Patch set 1 on the server with three remarks, and the local commit.
function bar(against?: string, current?: number) {
  const sets = [set(1, { posted: 3 }), set(2, { origin: 'local', gerritRef: undefined })];
  return mount(PatchSetBar, { props: { sets, current, against, gerrit: GERRIT, parents: [] } });
}

describe('the names of the versions', () => {
  it('calls a version of the server a Patchset, with its remarks', () => {
    const options = bar().findAll('#read-set option');

    expect(options[0].text()).toContain('Patchset 1');
    expect(options[0].text()).toContain('3 comments');
    expect(options[1].text()).toContain('Local');
    expect(options[1].text()).not.toContain('comment');
  });
});

describe('the remarks of another version', () => {
  it('says where they are when the version on the screen has none', () => {
    const wrapper = bar();

    expect(wrapper.find('.elsewhere').text()).toContain('3 Gerrit comments are on Patchset 1');
    wrapper.find('.elsewhere button').trigger('click');
    expect(wrapper.emitted('open')).toEqual([[2, 'ps:1']]);
  });

  it('says nothing once that version is on the left', () => {
    expect(bar('ps:1').find('.elsewhere').exists()).toBe(false);
  });

  it('says nothing on the version that holds them', () => {
    expect(bar(undefined, 1).find('.elsewhere').exists()).toBe(false);
  });
});
