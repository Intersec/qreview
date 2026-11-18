import { describe, expect, it } from 'vitest';
import type { Ui } from '@/api/types';
import { mode } from './colors';

const themes: Ui['themes'] = {
  night: { base: 'dark', colors: { page: '#000010' } },
  paper: { base: 'light', colors: {} },
};
const ui = (theme: string): Ui => ({ diff: 'unified', theme, themes });

describe('mode', () => {
  it('follows the system for system', () => {
    expect(mode(ui('system'))).toBeNull();
  });

  it('says light and dark outright', () => {
    expect(mode(ui('light'))).toBe('light');
    expect(mode(ui('dark'))).toBe('dark');
  });

  it('gives the base of a theme of the user', () => {
    expect(mode(ui('night'))).toBe('dark');
    expect(mode(ui('paper'))).toBe('light');
  });

  it('follows the system for a name it does not know', () => {
    expect(mode(ui('gone'))).toBeNull();
  });
});
