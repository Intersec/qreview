// @vitest-environment jsdom
//
// A thread on its card: the replies under the remark, the Done box that
// folds it, and the mark of a thread that waits for the reader.

import { mount } from '@vue/test-utils';
import { computed, nextTick } from 'vue';
import { describe, expect, it, vi } from 'vitest';
import CommentCard from './CommentCard.vue';
import { THREADS, type Threads } from '@/diff/threads';
import type { Comment } from '@/api/types';

function comment(id: string, extra: Partial<Comment> = {}): Comment {
  return {
    id,
    patchSet: 1,
    commit: 'abc',
    createdAt: `2026-10-03T10:0${id.length}:00Z`,
    updatedAt: '',
    scope: 'line',
    body: `Body of ${id}`,
    anchor: null,
    author: 'reader',
    parent: null,
    done: false,
    blocked: false,
    ...extra,
  };
}

function card(remark: Comment, replies: Comment[] = []) {
  const threads: Threads = {
    repliesOf: () => replies,
    reply: vi.fn(async () => {}),
    setDone: vi.fn(async () => {}),
    blockedChanges: computed(() => new Set<string>()),
    blockedFiles: computed(() => new Set<string>()),
  };
  const wrapper = mount(CommentCard, {
    props: { comment: remark, at: 'abc' },
    global: { provide: { [THREADS as symbol]: threads } },
  });
  return { wrapper, threads };
}

describe('a thread', () => {
  it('shows its replies under the remark, each with its author', () => {
    const { wrapper } = card(comment('a'), [
      comment('bb', { parent: 'a', author: 'agent', body: 'An answer.' }),
    ]);

    const reply = wrapper.get('.talk-reply');
    expect(reply.text()).toContain('Agent');
    expect(reply.text()).toContain('An answer.');
    expect(reply.classes()).toContain('talk-reply-agent');
  });

  it('marks a remark of the agent', () => {
    const { wrapper } = card(comment('a', { author: 'agent' }));

    expect(wrapper.get('article').classes()).toContain('talk-agent');
  });

  it('folds when done, and one click unfolds it without a change of state', async () => {
    const { wrapper, threads } = card(comment('a', { done: true, body: 'Rename b.\nMore.' }));

    expect(wrapper.find('.prose-comment').exists()).toBe(false);
    expect(wrapper.get('.talk-summary').text()).toBe('Rename b.');

    await wrapper.get('.talk-fold').trigger('click');

    expect(wrapper.find('.prose-comment').exists()).toBe(true);
    expect(threads.setDone).not.toHaveBeenCalled();
  });

  it('checks the Done box through the threads', async () => {
    const { wrapper, threads } = card(comment('a'));
    const box = wrapper.get('input[type="checkbox"]');

    (box.element as HTMLInputElement).checked = true;
    await box.trigger('change');

    expect(threads.setDone).toHaveBeenCalledWith('a', true);
  });

  it('never folds a thread that waits for the reader, and says so', () => {
    const { wrapper } = card(comment('a', { done: true }), [
      comment('bb', { parent: 'a', author: 'agent', blocked: true }),
    ]);

    expect(wrapper.get('article').classes()).toContain('talk-blocked');
    expect(wrapper.text()).toContain('waits for you');
    expect(wrapper.find('.prose-comment').exists()).toBe(true);
  });

  it('sends a reply', async () => {
    const { wrapper, threads } = card(comment('a'));

    const reply = wrapper.findAll('button').find((b) => b.text() === 'Reply');
    await reply?.trigger('click');
    await nextTick();
    await wrapper.get('textarea').setValue('Return it.');
    await wrapper.get('form').trigger('submit');

    expect(threads.reply).toHaveBeenCalledWith('a', 'Return it.');
  });

  it('takes no reply and no Done on a previous remark', () => {
    const { wrapper } = card(comment('a', { commit: 'old' }));

    expect(wrapper.find('input[type="checkbox"]').exists()).toBe(false);
    expect(wrapper.findAll('button').some((b) => b.text() === 'Reply')).toBe(false);
  });
});
