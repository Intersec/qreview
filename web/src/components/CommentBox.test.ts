// @vitest-environment jsdom
//
// The keys that save. Ctrl+S is the browser's own save, so the box has to
// prevent it, and the test that matters is the one where a plain `s` still
// reaches the text and still opens nothing.

import { mount } from '@vue/test-utils';
import { computed } from 'vue';
import { describe, expect, it } from 'vitest';
import CommentBox from './CommentBox.vue';
import { THREADS, type Threads } from '@/diff/threads';

/// Send a real key to the text area and say whether the browser kept it.
function press(area: HTMLTextAreaElement, key: string, ctrl: boolean, shift = false) {
  const event = new KeyboardEvent('keydown', {
    key,
    ctrlKey: ctrl,
    shiftKey: shift,
    bubbles: true,
    cancelable: true,
  });
  area.dispatchEvent(event);
  return event.defaultPrevented;
}

describe('the keys that save', () => {
  function box(body: string) {
    const wrapper = mount(CommentBox, { props: { label: 'Comment', start: body } });
    return { wrapper, area: wrapper.get('textarea').element as HTMLTextAreaElement };
  }

  it('saves on Ctrl+Enter', () => {
    const { wrapper, area } = box('A remark.');
    press(area, 'Enter', true);
    expect(wrapper.emitted('save')).toEqual([['A remark.', false, false, false]]);
  });

  it('saves on Ctrl+S, and keeps the browser out of it', () => {
    const { wrapper, area } = box('A remark.');
    expect(press(area, 's', true)).toBe(true);
    expect(wrapper.emitted('save')).toEqual([['A remark.', false, false, false]]);
  });

  it('holds the browser back even when there is nothing to save', () => {
    const { wrapper, area } = box('');
    expect(press(area, 's', true)).toBe(true);
    expect(wrapper.emitted('save')).toBeUndefined();
  });

  it('saves and asks on Ctrl+Shift+Enter, where the box can ask', () => {
    const wrapper = mount(CommentBox, {
      props: { label: 'Comment', start: 'A question?', askable: true },
    });
    const area = wrapper.get('textarea').element as HTMLTextAreaElement;

    press(area, 'Enter', true, true);

    expect(wrapper.emitted('save')).toEqual([['A question?', true, false, false]]);
  });

  it('never asks from a box that cannot', () => {
    const { wrapper, area } = box('A remark.');

    press(area, 'Enter', true, true);

    expect(wrapper.emitted('save')).toEqual([['A remark.', false, false, false]]);
  });

  it('leaves a plain s to the text', () => {
    const { wrapper, area } = box('A remark.');
    expect(press(area, 's', false)).toBe(false);
    expect(wrapper.emitted('save')).toBeUndefined();
  });
});

describe('the For Gerrit box', () => {
  function box(remark: boolean, onGerrit: boolean) {
    const threads = { onGerrit: computed(() => onGerrit) } as unknown as Threads;
    return mount(CommentBox, {
      props: { label: 'Comment', start: 'A remark.', remark },
      global: { provide: { [THREADS as symbol]: threads } },
    });
  }

  it('stages the remark when it is checked', async () => {
    const wrapper = box(true, true);
    await wrapper.get('input[type=checkbox]').setValue(true);
    await wrapper.get('form').trigger('submit');

    expect(wrapper.emitted('save')).toEqual([['A remark.', false, true, false]]);
  });

  it('offers to resolve the thread on a reply to Gerrit, once it goes there', async () => {
    const wrapper = mount(CommentBox, {
      props: { label: 'Reply', start: 'Done.', gerritReply: true },
    });
    expect(wrapper.text()).not.toContain('Resolve');

    await wrapper.findAll('input[type=checkbox]')[0]!.setValue(true);
    await wrapper.findAll('input[type=checkbox]')[1]!.setValue(true);
    await wrapper.get('form').trigger('submit');

    expect(wrapper.emitted('save')).toEqual([['Done.', false, true, true]]);
  });

  it('is not there on a version that Gerrit does not hold', () => {
    expect(box(true, false).find('input[type=checkbox]').exists()).toBe(false);
  });

  it('is not there on a reply', () => {
    expect(box(false, true).find('input[type=checkbox]').exists()).toBe(false);
  });
});
