// @vitest-environment jsdom

import { afterEach, describe, expect, it } from 'vitest';
import type { Ui } from '@/api/types';
import { applyTheme, mode, sheet } from './colors';

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

describe('sheet', () => {
  it('writes the colors of a theme under its base', () => {
    const shown = sheet({
      diff: 'unified',
      theme: 'night',
      themes: { night: { base: 'dark', colors: { page: '#000010', 'tok-keyword': '#ff79c6' } } },
    });

    expect(shown).toBe(
      "html:root[data-theme='dark'] {\n  --page: #000010;\n  --tok-keyword: #ff79c6;\n}\n",
    );
  });

  it('writes nothing for a built-in theme or a name it does not know', () => {
    expect(sheet(ui('dark'))).toBe('');
    expect(sheet(ui('system'))).toBe('');
    expect(sheet(ui('gone'))).toBe('');
  });

  it('drops what the server would never send', () => {
    const shown = sheet({
      diff: 'unified',
      theme: 'night',
      themes: {
        night: {
          base: 'dark',
          colors: { page: 'red;} body{x:y', 'x;}a{': '#fff', ink: 'url(x)', edge: '#abc' },
        },
      },
    });

    expect(shown).toBe("html:root[data-theme='dark'] {\n  --edge: #abc;\n}\n");
  });
});

const papers: Ui['themes'] = { paper: { base: 'light', colors: { page: '#fafaf0' } } };

afterEach(() => {
  document.getElementById('qreview-theme')?.remove();
  delete document.documentElement.dataset.theme;
});

describe('applyTheme', () => {
  it('sets the mode and the colors, in one style element', () => {
    applyTheme({ diff: 'unified', theme: 'paper', themes: papers });
    applyTheme({ diff: 'unified', theme: 'paper', themes: papers });

    expect(document.documentElement.dataset.theme).toBe('light');
    expect(document.querySelectorAll('#qreview-theme')).toHaveLength(1);
    expect(document.getElementById('qreview-theme')?.textContent).toContain('--page: #fafaf0;');
  });

  it('empties the colors and follows the system again', () => {
    applyTheme({ diff: 'unified', theme: 'paper', themes: papers });
    applyTheme({ diff: 'unified', theme: 'system', themes: papers });

    expect(document.documentElement.dataset.theme).toBeUndefined();
    expect(document.getElementById('qreview-theme')?.textContent).toBe('');
  });
});
