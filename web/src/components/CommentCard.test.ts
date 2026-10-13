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

function card(
  remark: Comment,
  replies: Comment[] = [],
  attach = false,
  moved: Record<string, { id: string; commit: string }> = {},
) {
  const threads: Threads = {
    repliesOf: () => replies,
    reply: vi.fn(async () => {}),
    ask: vi.fn(async () => {}),
    setDone: vi.fn(async () => {}),
    blockedChanges: computed(() => new Set<string>()),
    blockedFiles: computed(() => new Set<string>()),
    movedTo: (id: string) => moved[id],
    follow: vi.fn(async () => {}),
  };
  const wrapper = mount(CommentCard, {
    props: { comment: remark, at: 'abc' },
    global: { provide: { [THREADS as symbol]: threads } },
    ...(attach ? { attachTo: document.body } : {}),
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

    expect(threads.reply).toHaveBeenCalledWith('a', 'Return it.', false);
  });

  it('asks the agent about the thread, and says so', async () => {
    const { wrapper, threads } = card(comment('a'));

    const ask = wrapper.findAll('button').find((b) => b.text() === 'Ask agent');
    await ask?.trigger('click');

    expect(threads.ask).toHaveBeenCalledWith('a');
    expect(wrapper.text()).toContain('Asked');
  });

  it('offers no Ask under the last word of the agent', () => {
    const asks = (wrapper: ReturnType<typeof card>['wrapper']) =>
      wrapper.findAll('button').some((b) => b.text() === 'Ask agent');

    expect(asks(card(comment('a', { author: 'agent' })).wrapper)).toBe(false);
    expect(asks(card(comment('a'), [comment('b', { parent: 'a', author: 'agent' })]).wrapper)).toBe(
      false,
    );
    expect(
      asks(
        card(comment('a'), [
          comment('b', { parent: 'a', author: 'agent' }),
          comment('c', { parent: 'a' }),
        ]).wrapper,
      ),
    ).toBe(true);
  });

  it('saves a reply and asks with Ctrl+Shift+Enter', async () => {
    const { wrapper, threads } = card(comment('a'));

    const reply = wrapper.findAll('button').find((b) => b.text() === 'Reply');
    await reply?.trigger('click');
    await nextTick();
    const area = wrapper.get('textarea');
    await area.setValue('And this one?');
    await area.trigger('keydown', { key: 'Enter', ctrlKey: true, shiftKey: true });

    expect(threads.reply).toHaveBeenCalledWith('a', 'And this one?', true);
  });

  it('keeps a reply typed and not saved when the card comes back', async () => {
    localStorage.clear();
    const first = card(comment('a')).wrapper;
    await first
      .findAll('button')
      .find((b) => b.text() === 'Reply')
      ?.trigger('click');
    await nextTick();
    await first.get('textarea').setValue('Half an answer');
    first.unmount();

    const again = card(comment('a')).wrapper;

    expect(again.get('textarea').element.value).toBe('Half an answer');
    localStorage.clear();
  });

  it('keeps an edit typed and not saved, and drops one brought back', async () => {
    localStorage.clear();
    const first = card(comment('a')).wrapper;
    await first
      .findAll('button')
      .find((b) => b.text() === 'Edit')
      ?.trigger('click');
    await nextTick();
    await first.get('textarea').setValue('Body of a, better');
    first.unmount();

    const again = card(comment('a')).wrapper;
    expect(again.get('textarea').element.value).toBe('Body of a, better');
    await again.get('textarea').setValue('Body of a');
    again.unmount();

    expect(card(comment('a')).wrapper.find('textarea').exists()).toBe(false);
  });

  it('edits a reply of the reader in place, in a box that takes the keyboard', async () => {
    localStorage.clear();
    const { wrapper } = card(
      comment('a'),
      [
        comment('b', { parent: 'a', author: 'agent', body: 'An answer.' }),
        comment('c', { parent: 'a', body: 'My reply.' }),
      ],
      true,
    );
    const mine = wrapper.findAll('.talk-reply')[1]!;
    expect(wrapper.findAll('.talk-reply')[0]!.find('button').exists()).toBe(false);

    await mine
      .findAll('button')
      .find((b) => b.text() === 'Edit')
      ?.trigger('click');
    await nextTick();
    const area = wrapper.get('textarea');
    expect(area.element.value).toBe('My reply.');
    expect(document.activeElement).toBe(area.element);
    await area.setValue('My reply, better.');
    await wrapper.get('form').trigger('submit');

    expect(wrapper.emitted('edit')).toEqual([['c', 'My reply, better.']]);
    expect(wrapper.find('textarea').exists()).toBe(false);
    wrapper.unmount();
  });

  it('deletes a reply of the reader alone', async () => {
    const { wrapper } = card(comment('a'), [comment('c', { parent: 'a', body: 'My reply.' })]);

    await wrapper
      .get('.talk-reply')
      .findAll('button')
      .find((b) => b.text() === 'Delete')
      ?.trigger('click');

    expect(wrapper.emitted('remove')).toEqual([['c']]);
  });

  it('says where a moved thread went, and leads there', async () => {
    const { wrapper, threads } = card(comment('a@old', { commit: 'old' }), [], false, {
      'a@old': { id: 'a', commit: 'abc' },
    });

    const tag = wrapper.get('.talk-moved');
    expect(tag.text()).toContain('moved to abc');
    expect(wrapper.text()).not.toContain('previous');
    expect(wrapper.find('textarea').exists()).toBe(false);
    expect(wrapper.findAll('button').some((b) => b.text() === 'Reply')).toBe(false);
    await tag.trigger('click');

    expect(threads.follow).toHaveBeenCalledWith('a');
  });

  it('takes no reply and no Done on a previous remark', () => {
    const { wrapper } = card(comment('a', { commit: 'old' }));

    expect(wrapper.find('input[type="checkbox"]').exists()).toBe(false);
    expect(wrapper.findAll('button').some((b) => b.text() === 'Reply')).toBe(false);
  });
});
