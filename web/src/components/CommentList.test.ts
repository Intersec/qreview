// @vitest-environment jsdom
//
// The list of comments: the threads of reviewers first, then the open
// remarks, and the done and previous ones a click away.

import { mount } from '@vue/test-utils';
import { describe, expect, it } from 'vitest';
import CommentList from './CommentList.vue';
import type { ChangeComments, Comment } from '@/api/types';

function comment(id: string, extra: Partial<Comment> = {}): Comment {
  return {
    id,
    patchSet: 1,
    commit: 'abc',
    createdAt: '2026-11-17T09:00:00Z',
    updatedAt: '',
    scope: 'line',
    body: `Body of ${id}`,
    anchor: {
      file: 'src/a.c',
      side: 'new',
      startLine: 3,
      endLine: 3,
      startChar: null,
      endChar: null,
      blob: null,
      lineHash: null,
      context: [],
    },
    author: 'reader',
    parent: null,
    done: false,
    blocked: false,
    gerrit: null,
    ...extra,
  };
}

const written: ChangeComments[] = [
  {
    key: 'I1',
    subject: 'net: retry the read',
    commit: 'abc',
    comments: [
      comment('open'),
      comment('done', { done: true }),
      comment('sent', {
        gerrit: { state: 'sent', patchSet: 1, resolve: false, id: 'd1', sentAt: null },
      }),
    ],
    versions: [],
  },
];
const gerrit = [
  {
    key: 'I1',
    subject: 'net: retry the read',
    threads: [comment('gerrit:g1', { body: 'Name the child.', commit: 'old' })],
  },
];

function list() {
  return mount(CommentList, { props: { written, gerrit, openKey: null, height: 300 } });
}

const rows = (wrapper: ReturnType<typeof list>) =>
  wrapper.findAll('.list-row').map((row) => row.find('.list-gist').text());

describe('the list of comments', () => {
  it('puts the threads of reviewers first, and counts them as work', () => {
    const wrapper = list();

    expect(rows(wrapper)).toEqual(['Name the child.', 'Body of open']);
    expect(wrapper.get('.list-head').text()).toContain('To handle · 2');
    expect(wrapper.get('.list-group').text()).toContain('Gerrit');
  });

  it('keeps the done and the sent remarks a click away', async () => {
    const wrapper = list();

    await wrapper.get('.list-more').trigger('click');
    expect(rows(wrapper)).toEqual([
      'Name the child.',
      'Body of open',
      'Body of done',
      'Body of sent',
    ]);

    await wrapper.get('.list-more').trigger('click');
    expect(rows(wrapper)).toEqual(['Name the child.', 'Body of open']);
  });

  it('opens a thread of Gerrit with its change', async () => {
    const wrapper = list();

    await wrapper.findAll('.list-row')[0]!.trigger('click');
    expect(wrapper.emitted('go')?.[0]?.[0]).toBe('I1');
  });

  it('says while the threads of Gerrit are read', () => {
    const wrapper = mount(CommentList, {
      props: { written: [], gerrit: [], gerritReading: true, openKey: null, height: 300 },
    });

    expect(wrapper.get('.list-reading').text()).toContain('Reading the threads of Gerrit');
  });
});
