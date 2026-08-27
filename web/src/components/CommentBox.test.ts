// @vitest-environment jsdom
//
// The keys that save. Ctrl+S is the browser's own save, so the box has to
// prevent it, and the test that matters is the one where a plain `s` still
// reaches the text and still opens nothing.

import { mount } from '@vue/test-utils';
import { describe, expect, it } from 'vitest';
import CommentBox from './CommentBox.vue';

/// Send a real key to the text area and say whether the browser kept it.
function press(area: HTMLTextAreaElement, key: string, ctrl: boolean) {
  const event = new KeyboardEvent('keydown', {
    key,
    ctrlKey: ctrl,
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
    expect(wrapper.emitted('save')).toEqual([['A remark.']]);
  });

  it('saves on Ctrl+S, and keeps the browser out of it', () => {
    const { wrapper, area } = box('A remark.');
    expect(press(area, 's', true)).toBe(true);
    expect(wrapper.emitted('save')).toEqual([['A remark.']]);
  });

  it('holds the browser back even when there is nothing to save', () => {
    const { wrapper, area } = box('');
    expect(press(area, 's', true)).toBe(true);
    expect(wrapper.emitted('save')).toBeUndefined();
  });

  it('leaves a plain s to the text', () => {
    const { wrapper, area } = box('A remark.');
    expect(press(area, 's', false)).toBe(false);
    expect(wrapper.emitted('save')).toBeUndefined();
  });
});
