// @vitest-environment jsdom
//
// The dialog of Send sends on Enter alone, and only when Gerrit takes a
// draft from this run. It names each remark, and why one did not leave.

import { mount } from '@vue/test-utils';
import { afterEach, describe, expect, it } from 'vitest';
import SendDialog from './SendDialog.vue';
import type { Comment, GerritStatus } from '@/api/types';

function staged(id: string): { key: string; comment: Comment } {
  return {
    key: 'I1',
    comment: {
      id,
      patchSet: 1,
      commit: 'abc',
      createdAt: '',
      updatedAt: '',
      scope: 'line',
      body: `Remark ${id}\nmore`,
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
      gerrit: { state: 'staged', patchSet: 2, resolve: false, id: null, sentAt: null },
    },
  };
}

const ok: GerritStatus = { rest: 'ok', reason: null, base: 'https://review.example.com' };

function dialog(status: GerritStatus | null, failed = new Map<string, string>()) {
  return mount(SendDialog, {
    props: {
      series: false,
      staged: [staged('a')],
      status,
      failed,
      sending: false,
      subjectOf: () => undefined,
    },
    attachTo: document.body,
  });
}

function press(key: string) {
  window.dispatchEvent(new KeyboardEvent('keydown', { key, cancelable: true }));
}

let wrapper: ReturnType<typeof dialog> | null = null;
afterEach(() => wrapper?.unmount());

describe('the dialog of Send', () => {
  it('lists each remark with its place and its patch set, and sends on Enter', () => {
    wrapper = dialog(ok);
    expect(wrapper.text()).toContain('a.c:3');
    expect(wrapper.text()).toContain('patch set 2');
    expect(wrapper.text()).toContain('Remark a');

    press('Enter');
    expect(wrapper.emitted('send')).toHaveLength(1);
  });

  it('sends nothing without the password, and says why', () => {
    wrapper = dialog({
      rest: 'anonymous',
      reason: 'No gerrit.auth.command in the configuration of the user',
      base: 'https://review.example.com',
    });

    press('Enter');
    expect(wrapper.emitted('send')).toBeUndefined();
    expect(wrapper.text()).toContain('gerrit.auth.command');
  });

  it('says why Gerrit refused a remark', () => {
    wrapper = dialog(ok, new Map([['a', 'Gerrit answered 400']]));
    expect(wrapper.get('[role=alert]').text()).toBe('Gerrit answered 400');
  });

  it('closes on Escape', () => {
    wrapper = dialog(ok);
    press('Escape');
    expect(wrapper.emitted('close')).toHaveLength(1);
  });
});
