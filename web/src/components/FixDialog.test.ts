// @vitest-environment jsdom
//
// The dialog of Fix sends on Enter alone, and never while the agent waits
// for an answer.

import { mount } from '@vue/test-utils';
import { describe, expect, it } from 'vitest';
import FixDialog from './FixDialog.vue';
import type { Comment } from '@/api/types';

function comment(id: string): Comment {
  return {
    id,
    patchSet: 1,
    commit: 'abc',
    createdAt: '',
    updatedAt: '',
    scope: 'line',
    body: `First line of ${id}\nmore`,
    anchor: {
      file: 'a.c',
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
  };
}

function press(key: string) {
  window.dispatchEvent(new KeyboardEvent('keydown', { key, bubbles: true, cancelable: true }));
}

describe('the dialog of Fix', () => {
  it('lists the open threads and sends on Enter', () => {
    const wrapper = mount(FixDialog, { props: { open: [comment('a')], waiting: [] } });

    expect(wrapper.text()).toContain('a.c:3 First line of a');
    press('Enter');

    expect(wrapper.emitted('send')).toHaveLength(1);
    wrapper.unmount();
  });

  it('sends nothing on another key', () => {
    const wrapper = mount(FixDialog, { props: { open: [comment('a')], waiting: [] } });

    press('F');
    press('u');

    expect(wrapper.emitted('send')).toBeUndefined();
    wrapper.unmount();
  });

  it('jumps to the first thread that waits, and sends nothing', () => {
    const wrapper = mount(FixDialog, {
      props: { open: [comment('a')], waiting: [comment('b'), comment('c')] },
    });

    press('Enter');

    expect(wrapper.emitted('send')).toBeUndefined();
    expect(wrapper.emitted('jump')?.[0]?.[0]).toMatchObject({ id: 'b' });
    wrapper.unmount();
  });

  it('closes on Escape', () => {
    const wrapper = mount(FixDialog, { props: { open: [], waiting: [] } });

    press('Escape');

    expect(wrapper.emitted('close')).toHaveLength(1);
    wrapper.unmount();
  });
});
