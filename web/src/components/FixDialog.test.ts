// @vitest-environment jsdom
//
// The dialog of Fix sends on Enter alone, and never while the agent waits
// for an answer. On the series, it heads the threads of each change with
// its subject.

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

/// A thread of the change `key`.
function on(key: string, id: string) {
  return { key, comment: comment(id) };
}

const subjects: Record<string, string> = { I1: 'net: retry the read', I2: 'net: log the retry' };

function dialog(props: {
  series?: boolean;
  open: ReturnType<typeof on>[];
  waiting: ReturnType<typeof on>[];
}) {
  return mount(FixDialog, {
    props: { series: false, subjectOf: (key: string) => subjects[key], ...props },
  });
}

function press(key: string) {
  window.dispatchEvent(new KeyboardEvent('keydown', { key, bubbles: true, cancelable: true }));
}

describe('the dialog of Fix', () => {
  it('lists the open threads and sends on Enter', () => {
    const wrapper = dialog({ open: [on('I1', 'a')], waiting: [] });

    expect(wrapper.text()).toContain('a.c:3 First line of a');
    expect(wrapper.text()).not.toContain('net: retry the read');
    press('Enter');

    expect(wrapper.emitted('send')).toHaveLength(1);
    wrapper.unmount();
  });

  it('sends nothing on another key', () => {
    const wrapper = dialog({ open: [on('I1', 'a')], waiting: [] });

    press('F');
    press('u');

    expect(wrapper.emitted('send')).toBeUndefined();
    wrapper.unmount();
  });

  it('jumps to the first thread that waits, and sends nothing', () => {
    const wrapper = dialog({ open: [on('I1', 'a')], waiting: [on('I1', 'b'), on('I1', 'c')] });

    press('Enter');

    expect(wrapper.emitted('send')).toBeUndefined();
    expect(wrapper.emitted('jump')?.[0]?.[0]).toMatchObject({ key: 'I1', comment: { id: 'b' } });
    wrapper.unmount();
  });

  it('lists the threads of the series under the subject of each change', () => {
    const wrapper = dialog({
      series: true,
      open: [on('I1', 'a'), on('I1', 'b'), on('I2', 'c')],
      waiting: [],
    });

    expect(wrapper.get('[role="dialog"]').attributes('aria-label')).toBe('Fix the series');
    expect(wrapper.text()).toContain('3 open threads in 2 changes');
    expect(wrapper.findAll('.fix-list li').map((li) => li.text())).toEqual([
      'net: retry the read',
      'a.c:3 First line of a',
      'a.c:3 First line of b',
      'net: log the retry',
      'a.c:3 First line of c',
    ]);
    press('Enter');

    expect(wrapper.emitted('send')).toHaveLength(1);
    wrapper.unmount();
  });

  it('jumps to a thread that waits in another change of the series', () => {
    const wrapper = dialog({ series: true, open: [on('I1', 'a')], waiting: [on('I2', 'b')] });

    press('Enter');

    expect(wrapper.emitted('send')).toBeUndefined();
    expect(wrapper.emitted('jump')?.[0]?.[0]).toMatchObject({ key: 'I2', comment: { id: 'b' } });
    wrapper.unmount();
  });

  it('closes on Escape', () => {
    const wrapper = dialog({ open: [], waiting: [] });

    press('Escape');

    expect(wrapper.emitted('close')).toHaveLength(1);
    wrapper.unmount();
  });
});
