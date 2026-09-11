// @vitest-environment jsdom
//
// What the card at the end of the series says, and what its buttons ask for.

import { mount } from '@vue/test-utils';
import { describe, expect, it } from 'vitest';
import BoundaryCard from './BoundaryCard.vue';
import type { Boundary } from '@/api/types';

function boundary(over: Partial<Boundary> = {}): Boundary {
  return {
    kind: 'batch',
    commit: '1a2b3c4d5e6f7890abcdef',
    subject: 'net: retry the read',
    remaining: null,
    reason: '5 commits loaded',
    guessed: false,
    merge: null,
    ...over,
  };
}

function card(over: Partial<Boundary> = {}, batch = 5) {
  return mount(BoundaryCard, { props: { boundary: boundary(over), batch, busy: false } });
}

describe('what the card says', () => {
  it('names the commit the button would load', () => {
    // A hash alone says nothing. The reader decides from what is written.
    expect(card().text()).toContain('1a2b3c4d5e6f net: retry the read');
  });

  it('never promises the count it asks for', () => {
    expect(card().text()).toContain('Load up to 5 older');
    expect(card({}, 10).text()).toContain('Load up to 10 older');
  });

  it('offers the rest when the base is a known distance away', () => {
    const wrapper = card({ remaining: 25, reason: '50 commits loaded, 25 to the base' });

    expect(wrapper.text()).toContain('Load the rest (25)');
    wrapper.findAll('button')[1].trigger('click');
    expect(wrapper.emitted('more')).toEqual([[25]]);
  });

  it('asks for the batch size when the reader loads older', () => {
    const wrapper = card();

    wrapper.get('button').trigger('click');
    expect(wrapper.emitted('more')).toEqual([[]]);
  });

  it('leaves the rest out when it is no further than one batch', () => {
    // Two buttons that do the same thing are one button.
    expect(card({ remaining: 5 }).text()).not.toContain('Load the rest');
    expect(card({ remaining: 3 }).text()).not.toContain('Load the rest');
  });

  it('has nothing to load at the root', () => {
    const wrapper = card({ kind: 'root', commit: null, subject: null });

    expect(wrapper.findAll('button')).toHaveLength(0);
    expect(wrapper.text()).toContain('Start of the history');
  });
});
