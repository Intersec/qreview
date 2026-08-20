import { describe, expect, it } from 'vitest';
import { group } from './tree';
import type { FileEntry } from '@/api/types';

function file(path: string): FileEntry {
  return {
    path,
    oldPath: null,
    status: 'modified',
    language: '',
    binary: false,
    added: 1,
    removed: 1,
  };
}

describe('group', () => {
  it('says nothing about an empty change', () => {
    expect(group([])).toEqual([]);
  });

  it('puts a file at the top of the tree in a group with no directory', () => {
    const [g] = group([file('README.md')]);
    expect(g.dir).toBe('');
    expect(g.files[0].name).toBe('README.md');
  });

  it('says a directory once for the files under it', () => {
    const found = group([file('src/a.rs'), file('src/b.rs')]);
    expect(found).toHaveLength(1);
    expect(found[0].dir).toBe('src');
    expect(found[0].files.map((f) => f.name)).toEqual(['a.rs', 'b.rs']);
  });

  it('keeps the whole directory, however deep', () => {
    const [g] = group([file('crates/qreview/src/git/commit.rs')]);
    expect(g.dir).toBe('crates/qreview/src/git');
    expect(g.files[0].name).toBe('commit.rs');
  });

  it('never reorders the files git gave', () => {
    const found = group([file('a/one'), file('b/two'), file('a/three')]);
    expect(found.map((g) => g.dir)).toEqual(['a', 'b', 'a']);
  });
});
