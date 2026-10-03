// @vitest-environment jsdom
//
// Clean asks first: a click on the button deletes nothing.

import { mount } from '@vue/test-utils';
import { describe, expect, it } from 'vitest';
import CleanDialog from './CleanDialog.vue';
import type { Comment } from '@/api/types';

function done(id: string): Comment {
  return {
    id,
    patchSet: 1,
    commit: 'abc',
    createdAt: '',
    updatedAt: '',
    scope: 'file',
    body: `First line of ${id}\nmore`,
    anchor: null,
    author: 'reader',
    parent: null,
    done: true,
    blocked: false,
  };
}

function press(key: string) {
  window.dispatchEvent(new KeyboardEvent('keydown', { key, bubbles: true, cancelable: true }));
}

describe('the dialog of Clean', () => {
  it('lists what it deletes, and deletes on Enter', () => {
    const wrapper = mount(CleanDialog, { props: { done: [done('a'), done('b')] } });

    expect(wrapper.text()).toContain('2 done threads');
    expect(wrapper.text()).toContain('First line of a');
    press('Enter');

    expect(wrapper.emitted('clean')).toHaveLength(1);
    wrapper.unmount();
  });

  it('closes on Esc and deletes nothing', () => {
    const wrapper = mount(CleanDialog, { props: { done: [done('a')] } });

    press('Escape');

    expect(wrapper.emitted('close')).toHaveLength(1);
    expect(wrapper.emitted('clean')).toBeUndefined();
    wrapper.unmount();
  });
});
