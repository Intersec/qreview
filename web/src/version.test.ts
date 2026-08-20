import { describe, expect, it } from 'vitest';
import { version } from './version';

describe('version', () => {
  it('is a non-empty string', () => {
    expect(version).not.toBe('');
  });
});
