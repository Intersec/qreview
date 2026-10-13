// @vitest-environment jsdom
//
// The tab says it is the reader's turn until the page has the focus again.

import { afterEach, describe, expect, it, vi } from 'vitest';
import { nextTick } from 'vue';
import { questions, tell, turnMark, yourTurn } from './notify';

afterEach(() => {
  window.dispatchEvent(new Event('focus'));
  questions.value = 0;
  vi.restoreAllMocks();
  vi.useRealTimers();
});

describe('the turn of the reader', () => {
  it('says nothing to a reader who looks at the page', () => {
    vi.spyOn(document, 'hasFocus').mockReturnValue(true);

    tell('The fix is done', '');

    expect(yourTurn.value).toBe(false);
    expect(turnMark.value).toBe('');
  });

  it('turns in the title until the page has the focus again', async () => {
    vi.useFakeTimers();
    vi.spyOn(document, 'hasFocus').mockReturnValue(false);

    tell('The fix is done', '');
    await nextTick();
    const first = turnMark.value;
    vi.advanceTimersByTime(1000);

    expect(first).toBe('◑');
    expect(turnMark.value).toBe('◒');

    window.dispatchEvent(new Event('focus'));
    expect(turnMark.value).toBe('');
  });

  it('stays the turn of the reader while a question of the agent waits', async () => {
    vi.useFakeTimers();
    vi.spyOn(document, 'hasFocus').mockReturnValue(false);

    questions.value = 1;
    tell('The fix is done', '1 question waits for you.');
    await nextTick();
    window.dispatchEvent(new Event('focus'));
    vi.advanceTimersByTime(1000);

    expect(turnMark.value).not.toBe('');

    questions.value = 0;
    await nextTick();
    expect(turnMark.value).toBe('');
  });
});
