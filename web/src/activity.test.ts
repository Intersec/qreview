import { describe as group, expect, it } from 'vitest';
import type { Comment } from '@/api/types';
import { describe } from './activity';

const subjects: Record<string, string> = { I1: 'net: retry the read' };
const remark = {
  id: 'c-1',
  body: 'This loop never ends when read() returns 0.\nSee the man page.',
};
const subjectOf = (key: string) => subjects[key];
const commentOf = (_key: string, id: string) =>
  id === remark.id ? (remark as unknown as Comment) : undefined;

group('describe', () => {
  it('names the change of a fix by its subject', () => {
    const shown = describe({ kind: 'fix', key: 'I1', id: null }, subjectOf, commentOf);

    expect(shown.text).toBe('Agent fixing “net: retry the read”…');
  });

  it('names a fix of the series, which names no change', () => {
    const shown = describe({ kind: 'fix', key: '', id: null }, subjectOf, commentOf);

    expect(shown.text).toBe('Agent fixing the series…');
  });

  it('names a question by the first line of its remark, cut to fit', () => {
    const shown = describe({ kind: 'answer', key: 'I1', id: 'c-1' }, subjectOf, commentOf);

    expect(shown.text).toBe('Agent answering “This loop never ends when read() return…”…');
    expect(shown.title).toBe(remark.body);
  });

  it('names in general what the page does not hold', () => {
    expect(describe({ kind: 'fix', key: 'I9', id: null }, subjectOf, commentOf).text).toBe(
      'Agent fixing a change…',
    );
    expect(describe({ kind: 'answer', key: 'I1', id: 'c-9' }, subjectOf, commentOf).text).toBe(
      'Agent answering a thread…',
    );
  });
});
